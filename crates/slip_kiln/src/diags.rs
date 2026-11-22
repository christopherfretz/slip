//! Counting Kiln's diagnostics while they stream (`spec/model.md` §8
//! `Outcome`; `spec/landed/model/WP-010.md`).
//!
//! The parse runs Kiln with `-fdiagnostics-format=json`: one JSON object
//! per diagnostic per line (`kiln/spec/diag.md` "JSON output"), its notes
//! nested inside it. The counter reads each line as it completes and keeps
//! only the totals and the first error: memory is bounded by one line
//! (itself capped at [`LINE_CAP`]) whatever the volume of output.

use std::io::{self, Write};
use std::sync::{Arc, Mutex, PoisonError};

use slip_model::formats::clip_message;

/// The most of one line that is kept; the rest of a longer line is dropped
/// (the fields read come first in the object).
const LINE_CAP: usize = 64 * 1024;

/// What the diagnostics said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    /// Diagnostics of severity `error` or `fatal`.
    pub errors: u64,
    pub warnings: u64,
    /// The first error, `file:line:col: message` (or `message` without a
    /// location), at most [`slip_model::formats::MESSAGE_MAX`] bytes.
    pub first_error: Option<String>,
    /// The first error's id is a driver diagnostic (`drv_…`): the driver
    /// refused the command line.
    pub first_error_is_driver: bool,
    /// The first line that is not a JSON object (Kiln wrote something
    /// unstructured), at most [`slip_model::formats::MESSAGE_MAX`] bytes.
    pub first_other: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    tally: Tally,
    line: Vec<u8>,
}

impl State {
    fn end_line(&mut self) {
        let line = std::mem::take(&mut self.line);
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        match parse_line(text) {
            Some(d) => match d.severity.as_str() {
                "error" | "fatal" => {
                    self.tally.errors += 1;
                    if self.tally.first_error.is_none() {
                        self.tally.first_error_is_driver =
                            d.id.as_deref().is_some_and(|id| id.starts_with("drv_"));
                        let located = match (d.file, d.line, d.col) {
                            (Some(f), Some(l), Some(c)) => format!("{f}:{l}:{c}: {}", d.message),
                            _ => d.message,
                        };
                        self.tally.first_error = Some(clip_message(&located).to_owned());
                    }
                }
                "warning" => self.tally.warnings += 1,
                _ => {}
            },
            None => {
                if self.tally.first_other.is_none() {
                    self.tally.first_other = Some(clip_message(text).to_owned());
                }
            }
        }
    }
}

/// A [`Write`] that counts diagnostics; clones share the count.
#[derive(Clone, Debug, Default)]
pub(crate) struct Counter(Arc<Mutex<State>>);

impl Counter {
    /// The totals so far, the last line included even without its newline.
    pub(crate) fn finish(&self) -> Tally {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        state.end_line();
        state.tally.clone()
    }
}

impl Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut rest = buf;
        while let Some(at) = rest.iter().position(|&b| b == b'\n') {
            push_capped(&mut state.line, &rest[..at]);
            state.end_line();
            rest = &rest[at + 1..];
        }
        push_capped(&mut state.line, rest);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn push_capped(line: &mut Vec<u8>, bytes: &[u8]) {
    let room = LINE_CAP.saturating_sub(line.len());
    line.extend_from_slice(&bytes[..bytes.len().min(room)]);
}

/// The top-level fields of one diagnostic line that the tally uses.
#[derive(Debug, Default, PartialEq, Eq)]
struct Diagnostic {
    id: Option<String>,
    severity: String,
    file: Option<String>,
    line: Option<u64>,
    col: Option<u64>,
    message: String,
}

/// Reads the top-level `id`, `severity`, `file`, `line`, `col` and
/// `message` of a JSON object, stopping at the first value it cannot read
/// (a line cut at [`LINE_CAP`]). `None` if `text` does not start as an
/// object with a `severity` string.
fn parse_line(text: &str) -> Option<Diagnostic> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    p.ws();
    if !p.eat(b'{') {
        return None;
    }
    let mut out = Diagnostic::default();
    let mut severity = None;
    loop {
        p.ws();
        let Some(key) = p.string() else { break };
        p.ws();
        if !p.eat(b':') {
            break;
        }
        p.ws();
        let ok = match key.as_str() {
            "id" => p.opt_string().map(|v| out.id = v).is_ok(),
            "severity" => p.string().map(|v| severity = Some(v)).is_some(),
            "file" => p.opt_string().map(|v| out.file = v).is_ok(),
            "line" => p.opt_number().map(|v| out.line = v).is_ok(),
            "col" => p.opt_number().map(|v| out.col = v).is_ok(),
            "message" => p.string().map(|v| out.message = v).is_some(),
            _ => p.skip_value(),
        };
        if !ok {
            break;
        }
        p.ws();
        if !p.eat(b',') {
            break;
        }
    }
    out.severity = severity?;
    Some(out)
}

/// A value that is not complete or not of the expected kind.
struct Incomplete;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, word: &[u8]) -> bool {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            true
        } else {
            false
        }
    }

    /// A string or `null`.
    fn opt_string(&mut self) -> Result<Option<String>, Incomplete> {
        if self.literal(b"null") { Ok(None) } else { self.string().map(Some).ok_or(Incomplete) }
    }

    /// A non-negative integer or `null`.
    fn opt_number(&mut self) -> Result<Option<u64>, Incomplete> {
        if self.literal(b"null") {
            return Ok(None);
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        let digits = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| Incomplete)?;
        digits.parse().map(Some).map_err(|_| Incomplete)
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = self.s.get(self.i..self.i + 4)?;
        let v = u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?;
        self.i += 4;
        Some(v)
    }

    /// A JSON string (RFC 8259), decoded; an unpaired surrogate is U+FFFD.
    fn string(&mut self) -> Option<String> {
        if !self.eat(b'"') {
            return None;
        }
        let mut bytes = Vec::new();
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'"' => return Some(String::from_utf8_lossy(&bytes).into_owned()),
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi)
                                && self.s[self.i..].starts_with(b"\\u")
                            {
                                let save = self.i;
                                self.i += 2;
                                match self.hex4() {
                                    Some(lo) if (0xDC00..0xE000).contains(&lo) => char::from_u32(
                                        0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00),
                                    )
                                    .unwrap_or('\u{fffd}'),
                                    _ => {
                                        self.i = save;
                                        '\u{fffd}'
                                    }
                                }
                            } else {
                                char::from_u32(hi).unwrap_or('\u{fffd}')
                            }
                        }
                        _ => return None,
                    };
                    let mut buf = [0u8; 4];
                    bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                c => bytes.push(c),
            }
        }
    }

    /// Skips one value of any kind; `false` if it is not complete.
    fn skip_value(&mut self) -> bool {
        match self.peek() {
            Some(b'"') => self.string().is_some(),
            Some(b'{' | b'[') => {
                let mut depth = 0usize;
                while let Some(c) = self.peek() {
                    match c {
                        b'"' => {
                            if self.string().is_none() {
                                return false;
                            }
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                self.i += 1;
                                return true;
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
                false
            }
            Some(_) => {
                let start = self.i;
                while self.peek().is_some_and(|c| !matches!(c, b',' | b'}' | b']' | b' ')) {
                    self.i += 1;
                }
                self.i > start
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(text: &str, chunk: usize) -> Tally {
        let mut c = Counter::default();
        for part in text.as_bytes().chunks(chunk) {
            c.write_all(part).unwrap();
        }
        c.finish()
    }

    // spec/model.md §8 Outcome: errors (error and fatal), warnings, first error.
    #[test]
    fn s8_counts_and_first_error_in_any_split() {
        let text = concat!(
            r#"{"id":"pp_x","severity":"warning","file":"a.h","line":1,"col":2,"message":"w","group":null,"option":null,"spans":[],"fixits":[]}"#,
            "\n",
            r#"{"id":"sema_y","severity":"error","file":"a.cpp","line":3,"col":4,"message":"bad \"q\" é 😀","notes":[{"id":null,"severity":"error","message":"n"}],"spans":[],"fixits":[]}"#,
            "\n",
            r#"{"id":"sema_z","severity":"fatal","file":null,"line":null,"col":null,"message":"f"}"#,
            "\n",
            r#"{"id":null,"severity":"remark","message":"r"}"#,
        );
        for chunk in [1, 7, 4096] {
            let t = tally(text, chunk);
            assert_eq!(t.errors, 2, "nested notes are not counted");
            assert_eq!(t.warnings, 1);
            assert_eq!(t.first_error.as_deref(), Some("a.cpp:3:4: bad \"q\" é 😀"));
            assert!(!t.first_error_is_driver);
            assert_eq!(t.first_other, None);
        }
    }

    #[test]
    fn s8_driver_error_and_unstructured_text() {
        let t = tally(
            "kiln: something\n{\"id\":\"drv_unknown_argument\",\"severity\":\"error\",\"file\":null,\"line\":null,\"col\":null,\"message\":\"unknown argument: '-fx'\"}\n",
            3,
        );
        assert_eq!(t.errors, 1);
        assert!(t.first_error_is_driver);
        assert_eq!(t.first_error.as_deref(), Some("unknown argument: '-fx'"));
        assert_eq!(t.first_other.as_deref(), Some("kiln: something"));
    }

    // spec/model.md §8: the message is at most 4096 bytes, cut at a character boundary.
    #[test]
    fn s8_message_cut_and_line_capped() {
        let long = "é".repeat(5000);
        let line = format!("{{\"id\":\"x\",\"severity\":\"error\",\"message\":\"{long}\"}}\n");
        let t = tally(&line, 100);
        let m = t.first_error.unwrap();
        assert_eq!(m.len(), slip_model::formats::MESSAGE_MAX);
        assert!(m.chars().all(|c| c == 'é'));
        // A line longer than the cap still counts, from its prefix.
        let huge = "x".repeat(LINE_CAP * 2);
        let t = tally(&format!("{{\"severity\":\"warning\",\"message\":\"{huge}\"}}\n"), 1000);
        assert_eq!(t.warnings, 1);
    }
}
