//! Reads the bytes a shell prints and picks out what triggers need: the
//! prompt and command end marks the shell integration sends (OSC 133), the
//! bell, and, only when asked, the printable lines. Pure, with its state
//! carried between chunks because a read can end anywhere, even inside an
//! escape sequence. See docs/technical-design.md section 4.

/// The most bytes kept of one line. The rest of a longer line is dropped.
pub const MAX_LINE_BYTES: usize = 1024;

/// The most bytes kept of one OSC string. Longer ones are skipped unread.
const MAX_OSC_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// `OSC 133;A`: the shell is about to draw its prompt.
    PromptStart,
    /// `OSC 133;D;<code>`: a command ended. No code means 0.
    CommandEnd { exit: i32 },
    /// A BEL byte that is not the end of an OSC string.
    Bell,
}

/// What one chunk held.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub marks: Vec<Mark>,
    /// Complete lines, escapes removed, empty ones skipped. Always empty
    /// unless `want_lines` was true.
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Ground,
    /// After ESC.
    Esc,
    /// After ESC and an intermediate byte such as `(`; one more byte ends it.
    EscInter,
    /// Inside `ESC [`, until a final byte.
    Csi,
    /// Inside `ESC ]`, until BEL or ESC `\`.
    Osc,
    OscEsc,
    /// Inside DCS, SOS, PM or APC, until BEL or ESC `\`. Read and thrown away.
    Str,
    StrEsc,
}

#[derive(Debug, Default)]
pub struct Scanner {
    state: State,
    osc: Vec<u8>,
    osc_too_long: bool,
    line: Vec<u8>,
}

impl Scanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads one chunk. Marks come back in the order they appeared. Lines are
    /// only collected while `want_lines` is true, so a pane nobody is
    /// matching output on pays for the escape state machine and nothing else.
    pub fn feed(&mut self, bytes: &[u8], want_lines: bool) -> Scan {
        let mut scan = Scan::default();
        for &byte in bytes {
            self.step(byte, want_lines, &mut scan);
        }
        scan
    }

    fn step(&mut self, byte: u8, want_lines: bool, scan: &mut Scan) {
        match self.state {
            State::Ground => match byte {
                0x1b => self.state = State::Esc,
                0x07 => scan.marks.push(Mark::Bell),
                b'\n' | b'\r' => self.end_line(want_lines, scan),
                b'\t' => self.push_printable(b' ', want_lines),
                0x00..=0x1f | 0x7f => {}
                _ => self.push_printable(byte, want_lines),
            },
            State::Esc => match byte {
                b'[' => self.state = State::Csi,
                b']' => {
                    self.osc.clear();
                    self.osc_too_long = false;
                    self.state = State::Osc;
                }
                b'P' | b'X' | b'^' | b'_' => self.state = State::Str,
                0x20..=0x2f => self.state = State::EscInter,
                0x1b => {}
                // Any other byte makes a two byte escape such as `ESC =`.
                _ => self.state = State::Ground,
            },
            State::EscInter => {
                if !(0x20..=0x2f).contains(&byte) {
                    self.state = State::Ground;
                }
            }
            State::Csi => match byte {
                0x40..=0x7e => self.state = State::Ground,
                0x1b => self.state = State::Esc,
                _ => {}
            },
            State::Osc => match byte {
                0x07 => self.finish_osc(scan),
                0x1b => self.state = State::OscEsc,
                _ if self.osc.len() < MAX_OSC_BYTES => self.osc.push(byte),
                _ => self.osc_too_long = true,
            },
            State::OscEsc => {
                if byte == b'\\' {
                    self.finish_osc(scan);
                } else {
                    // ESC then something else: a new escape began, so the OSC was cut short.
                    self.state = State::Esc;
                    self.step(byte, want_lines, scan);
                }
            }
            State::Str => match byte {
                0x07 => self.state = State::Ground,
                0x1b => self.state = State::StrEsc,
                _ => {}
            },
            State::StrEsc => {
                if byte == b'\\' {
                    self.state = State::Ground;
                } else {
                    self.state = State::Esc;
                    self.step(byte, want_lines, scan);
                }
            }
        }
    }

    fn finish_osc(&mut self, scan: &mut Scan) {
        self.state = State::Ground;
        if self.osc_too_long {
            return;
        }
        if let Some(mark) = read_osc(&self.osc) {
            scan.marks.push(mark);
        }
    }

    fn push_printable(&mut self, byte: u8, want_lines: bool) {
        if want_lines && self.line.len() < MAX_LINE_BYTES {
            self.line.push(byte);
        }
    }

    fn end_line(&mut self, want_lines: bool, scan: &mut Scan) {
        if want_lines && !self.line.is_empty() {
            let text = String::from_utf8_lossy(&self.line);
            let text = text.trim();
            if !text.is_empty() {
                scan.lines.push(text.to_string());
            }
        }
        self.line.clear();
    }
}

/// Reads the marks we know out of an OSC string (the bytes between `ESC ]`
/// and the terminator). Everything else the shell sends, such as the working
/// directory (7) and the command text (7777), is ignored here.
fn read_osc(osc: &[u8]) -> Option<Mark> {
    let text = std::str::from_utf8(osc).ok()?;
    let mut parts = text.strip_prefix("133;")?.split(';');
    match parts.next()? {
        "A" => Some(Mark::PromptStart),
        "D" => Some(Mark::CommandEnd {
            exit: parts.next().and_then(|code| code.parse().ok()).unwrap_or(0),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(bytes: &[u8]) -> Vec<Mark> {
        Scanner::new().feed(bytes, false).marks
    }

    fn lines(bytes: &[u8]) -> Vec<String> {
        Scanner::new().feed(bytes, true).lines
    }

    #[test]
    fn reads_command_end_and_prompt_start() {
        assert_eq!(
            marks(b"\x1b]133;D;2\x07\x1b]133;A\x07"),
            vec![Mark::CommandEnd { exit: 2 }, Mark::PromptStart]
        );
    }

    #[test]
    fn command_end_without_a_code_is_zero_and_a_bad_code_too() {
        assert_eq!(marks(b"\x1b]133;D\x07"), vec![Mark::CommandEnd { exit: 0 }]);
        assert_eq!(
            marks(b"\x1b]133;D;x\x07"),
            vec![Mark::CommandEnd { exit: 0 }]
        );
        assert_eq!(
            marks(b"\x1b]133;D;-1\x07"),
            vec![Mark::CommandEnd { exit: -1 }]
        );
    }

    #[test]
    fn a_string_terminator_ends_an_osc_too() {
        assert_eq!(marks(b"\x1b]133;A\x1b\\"), vec![Mark::PromptStart]);
    }

    #[test]
    fn other_osc_strings_are_not_marks() {
        let sequence = b"\x1b]7;file://localhost/tmp\x07\x1b]7777;cmd;dHJ1ZQ==\x07\x1b]133;B\x07\x1b]0;title\x07";
        assert!(marks(sequence).is_empty());
    }

    #[test]
    fn a_real_prompt_cycle_gives_end_then_start() {
        let cycle = b"\x1b]7777;cmd;ZmFsc2U=\x07\x1b]133;D;1\x07\x1b]7;file://localhost/tmp\x07\x1b]133;A\x07";
        assert_eq!(
            marks(cycle),
            vec![Mark::CommandEnd { exit: 1 }, Mark::PromptStart]
        );
    }

    #[test]
    fn a_bare_bel_is_a_bell_but_the_bel_ending_an_osc_is_not() {
        assert_eq!(marks(b"\x07"), vec![Mark::Bell]);
        assert!(marks(b"\x1b]0;title\x07").is_empty());
        assert_eq!(marks(b"\x1b]0;title\x07\x07"), vec![Mark::Bell]);
        // A BEL inside a DCS string ends it and does not ring.
        assert!(marks(b"\x1bPq\x07").is_empty());
    }

    #[test]
    fn the_same_marks_come_out_however_the_chunks_are_cut() {
        let input = b"ab\x1b]133;D;3\x07cd\x07\x1b[31mef\x1b]133;A\x1b\\\x07";
        let expected = vec![
            Mark::CommandEnd { exit: 3 },
            Mark::Bell,
            Mark::PromptStart,
            Mark::Bell,
        ];
        assert_eq!(marks(input), expected);
        for cut in 0..=input.len() {
            let mut scanner = Scanner::new();
            let mut got = scanner.feed(&input[..cut], false).marks;
            got.extend(scanner.feed(&input[cut..], false).marks);
            assert_eq!(got, expected, "cut at {cut}");
        }
        let mut scanner = Scanner::new();
        let mut got = Vec::new();
        for byte in input {
            got.extend(scanner.feed(&[*byte], false).marks);
        }
        assert_eq!(got, expected, "one byte at a time");
    }

    #[test]
    fn an_osc_string_that_is_too_long_is_skipped_without_a_mark() {
        let mut input = b"\x1b]133;D;".to_vec();
        input.extend_from_slice(&[b'1'; 500]);
        input.push(0x07);
        assert!(marks(&input).is_empty());
        // The scanner is back to normal afterwards.
        let mut scanner = Scanner::new();
        assert!(scanner.feed(&input, false).marks.is_empty());
        assert_eq!(scanner.feed(b"\x07", false).marks, vec![Mark::Bell]);
    }

    #[test]
    fn lines_have_their_escapes_removed() {
        assert_eq!(
            lines(b"\x1b[31mred\x1b[0m text\r\nsecond\n"),
            vec!["red text", "second"]
        );
        assert_eq!(lines(b"x\x1b]7;file://localhost/tmp\x07y\n"), vec!["xy"]);
        assert_eq!(lines(b"a\x1b=b\x1b(Bc\x1bPdata\x1b\\d\n"), vec!["abcd"]);
        assert_eq!(lines(b"\x1b[?2004hprompt\x1b[?2004l\n"), vec!["prompt"]);
    }

    #[test]
    fn lines_are_only_collected_when_asked() {
        let mut scanner = Scanner::new();
        assert!(scanner.feed(b"hello\nworld\n", false).lines.is_empty());
        assert_eq!(scanner.feed(b"again\n", true).lines, vec!["again"]);
    }

    #[test]
    fn a_line_and_an_escape_can_be_split_across_chunks() {
        let mut scanner = Scanner::new();
        assert!(scanner.feed(b"hel", true).lines.is_empty());
        assert!(scanner.feed(b"lo \x1b[3", true).lines.is_empty());
        assert_eq!(
            scanner.feed(b"1mworld\nnext", true).lines,
            vec!["hello world"]
        );
        assert_eq!(scanner.feed(b"\n", true).lines, vec!["next"]);
    }

    #[test]
    fn a_character_split_across_chunks_stays_whole() {
        let bytes = "caf\u{e9}\n".as_bytes();
        let mut scanner = Scanner::new();
        assert!(scanner.feed(&bytes[..4], true).lines.is_empty());
        assert_eq!(scanner.feed(&bytes[4..], true).lines, vec!["caf\u{e9}"]);
    }

    #[test]
    fn carriage_returns_end_a_line_and_blank_lines_are_skipped() {
        assert_eq!(
            lines(b"10%\r20%\r\n\r\n   \n30%\n"),
            vec!["10%", "20%", "30%"]
        );
    }

    #[test]
    fn controls_other_than_tab_are_dropped_and_tab_is_a_space() {
        assert_eq!(lines(b"a\x08b\tc\x00d\n"), vec!["ab cd"]);
    }

    #[test]
    fn a_long_line_is_kept_to_the_byte_limit() {
        let mut input = vec![b'a'; MAX_LINE_BYTES * 3];
        input.push(b'\n');
        input.extend_from_slice(b"short\n");
        let got = lines(&input);
        assert_eq!(got[0].len(), MAX_LINE_BYTES);
        assert_eq!(got[1], "short");
    }

    #[test]
    fn an_escape_cut_short_by_another_escape_does_not_swallow_text() {
        // The OSC is abandoned when ESC [ starts a colour code.
        assert_eq!(lines(b"\x1b]0;ti\x1b[1mbold\n"), vec!["bold"]);
        assert_eq!(lines(b"\x1b[3\x1b[1mbold\n"), vec!["bold"]);
    }
}
