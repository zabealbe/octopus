//! The Program Status Protocol (OSC 7501): a program asks its terminal with a probe whether it reads status
//! reports, and once answered reports each change of what it is doing. A report naming an `id` describes one part of
//! the program, so only the reports without one say what the program is doing.

use alacritty_terminal::vte::{Params, Parser, Perform};
use serde::{Deserialize, Serialize};

/// What a terminal answers a program's probe, so the program reports to it.
pub(crate) const ANSWER: &[u8] = b"\x1b]7501;?\x1b\\";

/// What a program reports it is doing: `clear` withdraws its last report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProgramStatus {
    Working,
    Blocked,
    Done,
    Idle,
    Error,
    Clear,
}

/// What one stretch of a program's output said over the protocol.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Said {
    pub probed: bool,
    /// The last status it reported.
    pub status: Option<ProgramStatus>,
}

/// Reads a program's output for the protocol, carrying a sequence split across reads over to the next.
#[derive(Default)]
pub(crate) struct Listener {
    parser: Parser,
}

impl Listener {
    pub fn read(&mut self, output: &[u8]) -> Said {
        let mut said = Said::default();
        self.parser.advance(&mut said, output);
        said
    }
}

impl Perform for Said {
    fn osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
        let [b"7501", body] = params else { return };
        if *body == b"?" {
            self.probed = true;
            return;
        }
        let mut fields = body.split(|byte| *byte == b':');
        if fields.clone().any(|field| field.starts_with(b"id=")) {
            return;
        }
        let reported = match fields.find_map(|field| field.strip_prefix(b"state=")) {
            Some(b"working") => ProgramStatus::Working,
            Some(b"blocked") => ProgramStatus::Blocked,
            Some(b"done") => ProgramStatus::Done,
            Some(b"idle") => ProgramStatus::Idle,
            Some(b"error") => ProgramStatus::Error,
            Some(b"clear") => ProgramStatus::Clear,
            _ => return,
        };
        self.status = Some(reported);
    }

    fn print(&mut self, _: char) {}
    fn execute(&mut self, _: u8) {}
    fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: char) {}
    fn put(&mut self, _: u8) {}
    fn unhook(&mut self) {}
    fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, _: char) {}
    fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listener_reads_the_probe_and_the_last_status_across_split_reads() {
        let mut listener = Listener::default();
        assert_eq!(listener.read(b"hi\x1b]7501;?\x1b\\\x1b[c"), Said { probed: true, status: None });
        assert_eq!(listener.read(b"\x1b]7501;state=work"), Said::default());
        let said =
            listener.read(b"ing:app=claude-code\x07\x1b]7501;state=blocked:app=claude-code:kind=permission\x1b\\");
        assert_eq!(said, Said { probed: false, status: Some(ProgramStatus::Blocked) });
        assert_eq!(listener.read(b"\x1b]7501;state=idle:app=x\x07").status, Some(ProgramStatus::Idle));
        assert_eq!(listener.read(b"\x1b]7501;state=asleep\x07\x1b]0;title\x07"), Said::default());
    }

    #[test]
    fn a_report_naming_an_id_says_nothing_about_the_program() {
        let mut listener = Listener::default();
        let said = listener
            .read(b"\x1b]7501;state=working:app=opencode\x1b\\\x1b]7501;state=working:id=ses_1:app=opencode\x1b\\");
        assert_eq!(said.status, Some(ProgramStatus::Working));
        assert_eq!(listener.read(b"\x1b]7501;state=clear:id=ses_1\x1b\\"), Said::default());
    }
}
