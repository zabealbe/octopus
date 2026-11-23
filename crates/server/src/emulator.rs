//! The terminal host's live copy of each terminal's screen, which a client redraws whole on attaching and on every
//! resize.

use std::time::Duration;

use alacritty_terminal::Term;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Color, Processor, Rgb, Timeout};
use alacritty_terminal::vte::{Params, Parser, Perform};
use octopus_proto::term::{HISTORY_LINES, Screen};

/// Past this much, output that leaves the parser inside a sequence or a character is applied as it stands.
const UNSETTLED_BYTES: usize = 1 << 16;

pub(crate) struct Emulator {
    term: Term<VoidListener>,
    parser: Processor<Unsynced>,
    /// Output since the parser last stood between sequences and characters. The screen ends with it, so a client
    /// parses the output after the screen from where the emulator does.
    unsettled: Vec<u8>,
}

/// A client draws whole screens, so the emulator applies a synchronized update as it arrives.
#[derive(Default)]
struct Unsynced;

impl Timeout for Unsynced {
    fn set_timeout(&mut self, _: Duration) {}
    fn clear_timeout(&mut self) {}
    fn pending_timeout(&self) -> bool {
        false
    }
}

impl Emulator {
    pub fn new(cols: u16, rows: u16) -> Self {
        let config = Config { scrolling_history: HISTORY_LINES, kitty_keyboard: true, ..Config::default() };
        let term = Term::new(config, &size(cols, rows), VoidListener);
        Self { term, parser: Processor::new(), unsettled: Vec::new() }
    }

    pub(crate) fn advance(&mut self, output: &[u8]) {
        self.unsettled.extend_from_slice(output);
        if settled(&self.unsettled) || self.unsettled.len() > UNSETTLED_BYTES {
            self.parser.advance(&mut self.term, &self.unsettled);
            self.unsettled.clear();
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(size(cols, rows));
    }

    pub(crate) fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Mutable because reading the primary screen under the alternate one swaps them and back.
    pub fn screen(&mut self) -> Screen {
        let cols = u16::try_from(self.term.columns()).expect("the size came in as u16");
        let rows = u16::try_from(self.term.screen_lines()).expect("the size came in as u16");
        let mut drawing = Drawing::default();
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            let alternate = self.term.grid().clone();
            self.term.swap_alt();
            drawing.grid(self.term.grid());
            drawing.cursor(self.term.grid());
            self.term.swap_alt();
            *self.term.grid_mut() = alternate;
            drawing.text.push_str("\x1b[?1049h");
        }
        drawing.grid(self.term.grid());
        drawing.modes(*self.term.mode());
        drawing.saved_cursor(self.term.grid());
        drawing.cursor(self.term.grid());
        let bytes = [drawing.text.as_bytes(), &self.unsettled].concat();
        Screen { cols, rows, bytes: bytes.into() }
    }
}

fn size(cols: u16, rows: u16) -> TermSize {
    TermSize::new(cols.into(), rows.into())
}

/// Whether a parser that read `output` from rest prints the next byte as itself, as it does only between sequences
/// and characters.
fn settled(output: &[u8]) -> bool {
    let mut parser = Parser::new();
    parser.advance(&mut Actions::default(), output);
    let mut canary = Actions::default();
    parser.advance(&mut canary, b"x");
    canary == Actions { count: 1, printed: Some('x') }
}

#[derive(Default, PartialEq)]
struct Actions {
    count: usize,
    printed: Option<char>,
}

impl Perform for Actions {
    fn print(&mut self, c: char) {
        self.count += 1;
        self.printed = Some(c);
    }
    fn execute(&mut self, _: u8) {
        self.count += 1;
    }
    fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: char) {
        self.count += 1;
    }
    fn put(&mut self, _: u8) {
        self.count += 1;
    }
    fn unhook(&mut self) {
        self.count += 1;
    }
    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {
        self.count += 1;
    }
    fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, _: char) {
        self.count += 1;
    }
    fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {
        self.count += 1;
    }
}

const STYLE_FLAGS: [(Flags, &str); 11] = [
    (Flags::BOLD, "1"),
    (Flags::DIM, "2"),
    (Flags::ITALIC, "3"),
    (Flags::UNDERLINE, "4"),
    (Flags::DOUBLE_UNDERLINE, "4:2"),
    (Flags::UNDERCURL, "4:3"),
    (Flags::DOTTED_UNDERLINE, "4:4"),
    (Flags::DASHED_UNDERLINE, "4:5"),
    (Flags::INVERSE, "7"),
    (Flags::HIDDEN, "8"),
    (Flags::STRIKEOUT, "9"),
];

const SET_MODES: [(TermMode, &str); 12] = [
    (TermMode::ORIGIN, "\x1b[?6h"),
    (TermMode::APP_CURSOR, "\x1b[?1h"),
    (TermMode::APP_KEYPAD, "\x1b="),
    (TermMode::MOUSE_REPORT_CLICK, "\x1b[?1000h"),
    (TermMode::MOUSE_DRAG, "\x1b[?1002h"),
    (TermMode::MOUSE_MOTION, "\x1b[?1003h"),
    (TermMode::FOCUS_IN_OUT, "\x1b[?1004h"),
    (TermMode::UTF8_MOUSE, "\x1b[?1005h"),
    (TermMode::SGR_MOUSE, "\x1b[?1006h"),
    (TermMode::BRACKETED_PASTE, "\x1b[?2004h"),
    (TermMode::INSERT, "\x1b[4h"),
    (TermMode::LINE_FEED_NEW_LINE, "\x1b[20h"),
];

const RESET_MODES: [(TermMode, &str); 2] = [(TermMode::SHOW_CURSOR, "\x1b[?25l"), (TermMode::LINE_WRAP, "\x1b[?7l")];

/// The kitty keyboard protocol's flags, each with its bit in the `CSI > flags u` a program pushes.
const KEYBOARD_FLAGS: [(TermMode, u8); 5] = [
    (TermMode::DISAMBIGUATE_ESC_CODES, 1),
    (TermMode::REPORT_EVENT_TYPES, 2),
    (TermMode::REPORT_ALTERNATE_KEYS, 4),
    (TermMode::REPORT_ALL_KEYS_AS_ESC, 8),
    (TermMode::REPORT_ASSOCIATED_TEXT, 16),
];

/// The bytes that draw a screen on a terminal just reset, with the attributes and link they last set.
#[derive(Default)]
struct Drawing {
    text: String,
    style: Style,
    link: Option<Hyperlink>,
}

/// A cell's SGR attributes; a color is `None` at its default.
#[derive(PartialEq)]
struct Style {
    flags: Flags,
    fg: Option<Paint>,
    bg: Option<Paint>,
    underline: Option<Paint>,
}

#[derive(Clone, Copy, PartialEq)]
enum Paint {
    Indexed(u8),
    Rgb(Rgb),
}

impl Default for Style {
    fn default() -> Self {
        Self::of(&Cell::default())
    }
}

impl Style {
    fn of(cell: &Cell) -> Self {
        Self {
            flags: STYLE_FLAGS.iter().fold(Flags::empty(), |styled, (flag, _)| styled | (cell.flags & *flag)),
            fg: paint(cell.fg),
            bg: paint(cell.bg),
            underline: cell.underline_color().and_then(paint),
        }
    }

    fn sgr(&self) -> String {
        let colors = [
            self.fg.map(|paint| sgr_color(paint, 38, Some((30, 90)))),
            self.bg.map(|paint| sgr_color(paint, 48, Some((40, 100)))),
            self.underline.map(|paint| sgr_color(paint, 58, None)),
        ];
        let flags = STYLE_FLAGS.iter().filter(|(flag, _)| self.flags.contains(*flag)).map(|(_, code)| code.to_string());
        let codes = std::iter::once("0".to_string()).chain(flags);
        format!("\x1b[{}m", codes.chain(colors.into_iter().flatten()).collect::<Vec<_>>().join(";"))
    }
}

/// The 16 named colors as their palette indexes; the default foreground and background, and the colors alacritty
/// derives only to render, as `None`.
fn paint(color: Color) -> Option<Paint> {
    match color {
        Color::Named(named) => u8::try_from(named as usize).ok().filter(|&index| index < 16).map(Paint::Indexed),
        Color::Indexed(index) => Some(Paint::Indexed(index)),
        Color::Spec(rgb) => Some(Paint::Rgb(rgb)),
    }
}

/// `basic` holds the codes that start the 8 normal and the 8 bright colors, where the layer has them.
fn sgr_color(paint: Paint, extended: u8, basic: Option<(u8, u8)>) -> String {
    match (paint, basic) {
        (Paint::Indexed(index), Some((normal, _))) if index < 8 => (normal + index).to_string(),
        (Paint::Indexed(index), Some((_, bright))) if index < 16 => (bright + index - 8).to_string(),
        (Paint::Indexed(index), _) => format!("{extended};5;{index}"),
        (Paint::Rgb(rgb), _) => format!("{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b),
    }
}

impl Drawing {
    /// Every line from the oldest in history, each ending at its last cell that differs from a blank unless it wraps
    /// into the next.
    fn grid(&mut self, grid: &Grid<Cell>) {
        self.text.push_str("\x1b[H");
        let (last_column, bottom) = (grid.last_column(), grid.bottommost_line());
        for line in grid.topmost_line().0..=bottom.0 {
            let row = &grid[Line(line)];
            let wraps = row[last_column].flags.contains(Flags::WRAPLINE);
            let blank = Cell::default();
            let end = if wraps {
                grid.columns()
            } else {
                (0..grid.columns()).rev().find(|&column| row[Column(column)] != blank).map_or(0, |at| at + 1)
            };
            for column in 0..end {
                let cell = &row[Column(column)];
                if !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    self.cell(cell);
                }
            }
            if !wraps && Line(line) != bottom {
                self.restyle(&blank);
                self.text.push_str("\r\n");
            }
        }
    }

    fn saved_cursor(&mut self, grid: &Grid<Cell>) {
        let saved = &grid.saved_cursor;
        self.goto(saved.point.line, saved.point.column);
        self.restyle(&saved.template);
        self.text.push_str("\x1b7");
    }

    /// A cursor waiting to wrap stands past the last cell, where only printing that cell again puts it.
    fn cursor(&mut self, grid: &Grid<Cell>) {
        let cursor = &grid.cursor;
        let row = &grid[cursor.point.line];
        let mut column = cursor.point.column;
        if cursor.input_needs_wrap && row[column].flags.contains(Flags::WIDE_CHAR_SPACER) {
            column -= 1;
        }
        self.goto(cursor.point.line, column);
        if cursor.input_needs_wrap {
            self.cell(&row[column]);
        }
        self.restyle(&cursor.template);
    }

    fn goto(&mut self, line: Line, column: Column) {
        self.text.push_str(&format!("\x1b[{};{}H", line.0 + 1, column.0 + 1));
    }

    fn modes(&mut self, mode: TermMode) {
        for (flag, set) in SET_MODES {
            if mode.contains(flag) {
                self.text.push_str(set);
            }
        }
        for (flag, reset) in RESET_MODES {
            if !mode.contains(flag) {
                self.text.push_str(reset);
            }
        }
        let keyboard: u8 = KEYBOARD_FLAGS.iter().filter(|(flag, _)| mode.contains(*flag)).map(|(_, bit)| bit).sum();
        if keyboard != 0 {
            self.text.push_str(&format!("\x1b[>{keyboard}u"));
        }
    }

    /// A tab stands in its first cell for copying; the cells it spans follow as blanks.
    fn cell(&mut self, cell: &Cell) {
        self.restyle(cell);
        self.text.push(if cell.c == '\t' { ' ' } else { cell.c });
        self.text.extend(cell.zerowidth().into_iter().flatten());
    }

    fn restyle(&mut self, cell: &Cell) {
        let link = cell.hyperlink();
        if link != self.link {
            match &link {
                Some(link) => self.text.push_str(&format!("\x1b]8;id={};{}\x1b\\", link.id(), link.uri())),
                None => self.text.push_str("\x1b]8;;\x1b\\"),
            }
            self.link = link;
        }
        let style = Style::of(cell);
        if style != self.style {
            self.text.push_str(&style.sgr());
            self.style = style;
        }
    }
}
