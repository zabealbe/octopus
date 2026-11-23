use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use nix::libc;
use nix::unistd::Pid;
use octopus_proto::Error;
use octopus_proto::term::{Piece, Screen};
use portable_pty::{CommandBuilder, MasterPty, PtyPair, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};

use crate::emulator::Emulator;
use crate::program_status::{ANSWER, Listener, ProgramStatus};
use crate::{lock, now, report};

const LIVE_PIECES: usize = 1024;
const READ_BYTES: usize = 8192;
const KILL_GRACE: Duration = Duration::from_secs(5);
/// A limit in the millions would make every spawn walk them all.
const MOST_DESCRIPTORS: libc::c_long = 65_536;
/// Every terminal starts at this size, and a restarted server puts every terminal it adopts back to it.
pub(crate) const INITIAL_SIZE: (u16, u16) = (80, 24);

/// The terminal's screen and the pieces it sends each attached client, kept in step under one lock.
struct Output {
    emulator: Emulator,
    /// `None` once the program's output has ended.
    live: Option<broadcast::Sender<Piece>>,
}

impl Output {
    fn new(live: Option<broadcast::Sender<Piece>>) -> Self {
        Self { emulator: Emulator::new(INITIAL_SIZE.0, INITIAL_SIZE.1), live }
    }

    fn add(&mut self, bytes: &[u8]) {
        self.emulator.advance(bytes);
        self.send(Piece::Output(bytes.into()));
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.emulator.resize(cols, rows);
        let screen = self.emulator.screen();
        self.send(Piece::Screen(screen));
    }

    fn send(&self, piece: Piece) {
        if let Some(live) = &self.live {
            let _no_receivers = live.send(piece);
        }
    }
}

pub(crate) struct Attachment {
    pub screen: Screen,
    /// `None` once the terminal's output has ended.
    pub live: Option<broadcast::Receiver<Piece>>,
}

/// A PTY and its process, owned by the terminal host.
pub struct Pty {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    /// Leads its own session and group, so signalling the group reaches everything it started. Dropped right after
    /// the reap, narrowing the window for a signal to reach a process that reuses the id.
    process: Mutex<Option<Pid>>,
    output: Mutex<Output>,
    /// When the terminal last wrote output, or started.
    output_at: AtomicI64,
    /// What the program last reported it is doing, once it has.
    status: watch::Sender<Option<ProgramStatus>>,
    exited: watch::Sender<bool>,
}

/// What tells whether a terminal's program is still at work.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Activity {
    pub output_at: i64,
    /// Whether a process descending from the program runs outside the program's process group, as a shell an agent
    /// starts for a command does; the processes of the agent itself, such as its MCP servers, stay in its group.
    pub detached_descendant: bool,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl Pty {
    pub fn spawn(argv: &[String], cwd: &Path, env: &[(String, String)]) -> Result<Arc<Self>, Error> {
        let pair = native_pty_system().openpty(size(INITIAL_SIZE.0, INITIAL_SIZE.1)).map_err(Error::internal)?;
        let mut command = CommandBuilder::from_argv(argv.iter().map(Into::into).collect());
        command.cwd(cwd);
        for (key, value) in env {
            command.env(key, value);
        }
        let child = start(&pair, command).map_err(Error::internal)?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(Error::internal)?;
        let writer = pair.master.take_writer().map_err(Error::internal)?;
        let (live, _) = broadcast::channel(LIVE_PIECES);
        let pty = Arc::new(Self {
            writer: Mutex::new(writer),
            master: Mutex::new(Some(pair.master)),
            process: Mutex::new(Some(child)),
            output: Mutex::new(Output::new(Some(live))),
            output_at: AtomicI64::new(now()),
            status: watch::Sender::new(None),
            exited: watch::Sender::new(false),
        });
        let (pump, reaper) = (Arc::clone(&pty), Arc::clone(&pty));
        std::thread::spawn(move || pump.pump(reader));
        std::thread::spawn(move || reaper.reap(child));
        Ok(pty)
    }

    /// Answers a probe before any client sees it, so the answer beats a client's answer to a query sent with it.
    fn pump(&self, mut reader: Box<dyn Read + Send>) {
        let mut buffer = [0u8; READ_BYTES];
        let mut listener = Listener::default();
        while let Ok(read) = reader.read(&mut buffer)
            && read > 0
        {
            self.output_at.store(now(), Ordering::Relaxed);
            let said = listener.read(&buffer[..read]);
            if said.probed
                && let Err(error) = self.write(ANSWER)
            {
                report!("answer a program's status probe: {error}");
            }
            if let Some(status) = said.status {
                self.status.send_replace(Some(status));
            }
            lock(&self.output).add(&buffer[..read]);
        }
        lock(&self.output).live = None;
    }

    fn reap(&self, child: Pid) {
        if let Err(error) = nix::sys::wait::waitpid(child, None) {
            report!("waiting on a terminal's process: {error}");
        }
        lock(&self.process).take();
        lock(&self.master).take();
        self.exited.send_replace(true);
    }

    pub fn attach(&self) -> Attachment {
        let mut output = lock(&self.output);
        Attachment { screen: output.emulator.screen(), live: output.live.as_ref().map(broadcast::Sender::subscribe) }
    }

    pub fn running(&self) -> bool {
        !*self.exited.borrow()
    }

    pub(crate) fn program(&self) -> Option<Pid> {
        *lock(&self.process)
    }

    /// Reads the process table, so it blocks.
    pub(crate) fn activity(&self) -> Result<Activity, Error> {
        let program = (*lock(&self.process)).ok_or_else(|| Error::conflict("the terminal has exited"))?;
        let table = std::process::Command::new("ps")
            .args(["-A", "-o", "pid=,ppid=,pgid="])
            .output()
            .map_err(|error| Error::internal(format!("ps: {error}")))?;
        if !table.status.success() {
            return Err(Error::internal(format!("ps: {}", table.status)));
        }
        let mut children: HashMap<i32, Vec<(i32, i32)>> = HashMap::new();
        for line in String::from_utf8_lossy(&table.stdout).lines() {
            if let [Ok(pid), Ok(parent), Ok(group)] =
                line.split_whitespace().map(str::parse::<i32>).collect::<Vec<_>>()[..]
            {
                children.entry(parent).or_default().push((pid, group));
            }
        }
        let mut pending = vec![program.as_raw()];
        let mut detached_descendant = false;
        while let Some(parent) = pending.pop()
            && !detached_descendant
        {
            for &(pid, group) in children.get(&parent).into_iter().flatten() {
                detached_descendant |= group != program.as_raw();
                pending.push(pid);
            }
        }
        Ok(Activity { output_at: self.output_at.load(Ordering::Relaxed), detached_descendant })
    }

    pub(crate) fn bracketed_paste(&self) -> bool {
        lock(&self.output).emulator.bracketed_paste()
    }

    pub(crate) fn statuses(&self) -> watch::Receiver<Option<ProgramStatus>> {
        self.status.subscribe()
    }

    pub async fn exit(&self) {
        let mut exited = self.exited.subscribe();
        let _sender_lives_in_self = exited.wait_for(|exited| *exited).await.map(drop);
    }

    pub fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        let mut writer = lock(&self.writer);
        writer.write_all(bytes).and_then(|()| writer.flush()).map_err(Error::internal)
    }

    /// Holds the output while it resizes, so whatever the program redraws at the new size follows the new screen.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), Error> {
        let mut output = lock(&self.output);
        match lock(&self.master).as_ref() {
            Some(master) => master.resize(size(cols, rows)).map_err(Error::internal)?,
            None => return Err(Error::conflict("the terminal has exited")),
        }
        output.resize(cols, rows);
        Ok(())
    }

    pub(crate) async fn terminate(self: Arc<Self>) {
        self.signal(nix::sys::signal::Signal::SIGTERM);
        if tokio::time::timeout(KILL_GRACE, self.exit()).await.is_err() {
            self.kill();
        }
    }

    fn signal(&self, signal: nix::sys::signal::Signal) {
        if let Some(pid) = *lock(&self.process)
            && let Err(error) = nix::sys::signal::killpg(pid, signal)
        {
            report!("send {signal} to {pid}: {error}");
        }
    }

    pub fn kill(&self) {
        self.signal(nix::sys::signal::Signal::SIGKILL);
    }
}

/// Starts the program as the leader of a new session whose controlling terminal is the PTY, so it leads the terminal's
/// foreground group. A program the OS refuses to run fails naming why; a file it can't tell is a program runs through
/// `sh`, which reports it in the terminal.
#[allow(unsafe_code, reason = "signals, setsid and the controlling-terminal ioctl have no safe binding before exec")]
fn start(pair: &PtyPair, mut command: CommandBuilder) -> anyhow::Result<Pid> {
    command.env("SHELL", command.get_shell());
    let argv = command.get_argv();
    let cwd = command.get_cwd().expect("a terminal has a working directory");
    let program = which::which_in(&argv[0], command.get_env("PATH"), cwd);
    let program = program.with_context(|| format!("find {:?}", argv[0]))?;
    if !Path::new(cwd).is_dir() {
        bail!("start {}: {} is not a directory", program.display(), Path::new(cwd).display());
    }
    let tty = pair.master.tty_name().context("find the terminal's device")?;
    let open = File::options().read(true).write(true).custom_flags(libc::O_NOCTTY).open(&tty);
    let terminal = open.with_context(|| format!("open {}", tty.display()))?;
    let mut spawning = Command::new(&program);
    spawning
        .arg0(&argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .env_clear()
        .envs(command.iter_full_env_as_str())
        .stdin(terminal.try_clone()?)
        .stdout(terminal.try_clone()?)
        .stderr(terminal);
    // SAFETY: the closure calls only async-signal-safe syscall wrappers and allocates nothing.
    unsafe {
        spawning.pre_exec(|| {
            for signal in [libc::SIGCHLD, libc::SIGHUP, libc::SIGINT, libc::SIGQUIT, libc::SIGTERM, libc::SIGALRM] {
                libc::signal(signal, libc::SIG_DFL);
            }
            if libc::setsid() == -1 || libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // Another terminal may be mid-spawn, with descriptors not yet marked. Marking rather than closing keeps the
            // pipe std reports an exec failure on.
            let last = libc::sysconf(libc::_SC_OPEN_MAX).clamp(0, MOST_DESCRIPTORS) as libc::c_int;
            for descriptor in 3..last {
                libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC);
            }
            Ok(())
        });
    }
    let child = spawning.spawn().map_err(|error| anyhow!("start {}: {error}", program.display()))?;
    Ok(Pid::from_raw(i32::try_from(child.id())?))
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::Term;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::term::test::TermSize;
    use alacritty_terminal::term::{Config, TermMode};
    use alacritty_terminal::vte::ansi::Processor;

    use super::*;

    /// A client: a terminal that reads the pieces as a client does, so it shows what that client draws.
    struct Client {
        term: Term<VoidListener>,
        parser: Processor,
    }

    impl Client {
        fn new() -> Self {
            let config = Config { kitty_keyboard: true, ..Config::default() };
            Self { term: Term::new(config, &TermSize::new(1, 1), VoidListener), parser: Processor::new() }
        }

        fn read(&mut self, piece: &Piece) {
            match piece {
                Piece::Screen(Screen { cols, rows, bytes }) => {
                    *self = Self::new();
                    self.term.resize(TermSize::new((*cols).into(), (*rows).into()));
                    self.parser.advance(&mut self.term, bytes);
                }
                Piece::Output(bytes) => self.parser.advance(&mut self.term, bytes),
            }
        }

        /// Every line from the oldest, trailing blanks trimmed.
        fn lines(&self) -> Vec<String> {
            let grid = self.term.grid();
            (grid.topmost_line().0..=grid.bottommost_line().0)
                .map(|line| {
                    let row = &grid[Line(line)];
                    (0..grid.columns()).map(|column| row[Column(column)].c).collect::<String>().trim_end().to_string()
                })
                .collect()
        }

        fn cursor(&self) -> (i32, usize) {
            let point = self.term.grid().cursor.point;
            (point.line.0, point.column.0)
        }
    }

    fn output() -> Output {
        Output::new(None)
    }

    fn attach(output: &mut Output) -> Client {
        let mut client = Client::new();
        client.read(&Piece::Screen(output.emulator.screen()));
        client
    }

    #[test]
    fn output_written_wide_reflows_into_the_screen_a_narrower_attach_draws() {
        let mut output = output();
        output.resize(110, 30);
        let long = "0123456789".repeat(10);
        output.add(format!("\x1b[1;31m{long}\x1b[0m\r\nnext\r\n\x1b[?1h$ ").as_bytes());
        output.resize(47, 12);

        let client = attach(&mut output);
        let Screen { cols, rows, .. } = output.emulator.screen();
        assert_eq!((cols, rows), (47, 12));
        let lines = client.lines();
        let drawn = lines.iter().position(|line| line.starts_with("0123")).expect("the long line");
        assert_eq!(lines[drawn..drawn + 5], [&long[..47], &long[47..94], &long[94..], "next", "$"]);
        let grid = client.term.grid();
        let wrapped = &grid[Line(i32::try_from(drawn + 1).unwrap() - i32::try_from(grid.history_size()).unwrap())];
        assert!(wrapped[Column(0)].flags.contains(Flags::BOLD), "attributes survive the reflow");
        assert_eq!(client.cursor().1, 2);
        assert!(client.term.mode().contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn a_client_attaching_after_a_program_pushed_its_keyboard_flags_reads_them() {
        let mut output = output();
        output.add(b"\x1b[?1049h\x1b[>7u");
        let client = attach(&mut output);
        let flags = TermMode::DISAMBIGUATE_ESC_CODES | TermMode::REPORT_EVENT_TYPES | TermMode::REPORT_ALTERNATE_KEYS;
        assert_eq!(client.term.mode().intersection(TermMode::KITTY_KEYBOARD_PROTOCOL), flags);
    }

    #[test]
    fn a_screen_draws_a_link_the_way_the_program_wrote_it() {
        let mut output = output();
        let link = "\x1b]8;id=pr;https://example.com/1\x1b\\#1\x1b]8;;\x1b\\ after";
        output.add(link.as_bytes());

        let Screen { bytes, .. } = output.emulator.screen();
        assert!(String::from_utf8_lossy(&bytes).contains(link), "a link ends with no parameters, as xterm.js reads it");
    }

    #[test]
    fn a_character_split_across_an_attach_arrives_whole() {
        let mut output = output();
        output.add("a\x1b[38;5".as_bytes());
        output.add(b";245mb\xe2\x94");
        let mut client = attach(&mut output);
        let rest = b"\x80c";
        output.add(rest);
        client.read(&Piece::Output(rest[..].into()));
        assert_eq!(client.lines()[0], "ab─c");
        assert_eq!(attach(&mut output).lines()[0], "ab─c");
    }

    #[test]
    fn a_full_screen_program_redraws_over_the_shell_it_returns_to_and_a_saved_cursor_survives_an_attach() {
        let mut output = output();
        output.add(b"$ vim\r\n\x1b[?1049h\x1b[Hfile\x1b[24;1H-- INSERT --\x1b[1;5H");
        let mut client = attach(&mut output);
        assert!(client.term.mode().contains(TermMode::ALT_SCREEN));
        assert_eq!(client.lines()[0], "file");
        assert_eq!(client.lines()[23], "-- INSERT --");
        assert_eq!(client.cursor(), (0, 4));
        let leave = b"\x1b[?1049l";
        output.add(leave);
        client.read(&Piece::Output(leave[..].into()));
        assert_eq!(client.lines()[0], "$ vim");
        assert_eq!(client.cursor(), (1, 0));
        assert_eq!(attach(&mut output).lines()[..2], ["$ vim", ""]);

        output.add(b"\x1b[3;5H\x1b7\x1b[H");
        let mut client = attach(&mut output);
        assert_eq!(client.cursor(), (0, 0));
        client.read(&Piece::Output(b"\x1b8"[..].into()));
        assert_eq!(client.cursor(), (2, 4));
    }
}
