//! Splitting a command string into words (`spec/cdb.md` §3), and the GNU
//! response-file variant (`spec/cdb.md` §4). Work package WP-003.

use std::fmt;

/// A command string that cannot be split (`spec/cdb.md` §3, §4).
///
/// The entry it came from is excluded with `malformed_entry`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitError {
    /// Byte offset in the input of the character at fault: the opening quote
    /// that is never closed, the trailing backslash, or 0 for an input with
    /// no words.
    pub offset: usize,
    /// What is wrong, in a few words.
    pub message: &'static str,
}

impl fmt::Display for SplitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for SplitError {}

/// The message of a quote that is never closed.
pub const UNCLOSED_DOUBLE_QUOTE: &str = "double quote is never closed";
/// The message of a single quote that is never closed.
pub const UNCLOSED_SINGLE_QUOTE: &str = "single quote is never closed";
/// The message of a backslash that is the last character.
pub const TRAILING_BACKSLASH: &str = "backslash is the last character";
/// The message of a command string with no words.
pub const NO_WORDS: &str = "command has no words";

/// Splits a `command` string of a JSON compilation database
/// (`spec/cdb.md` §3).
///
/// A string with no words is an error.
pub fn split_command(s: &str) -> Result<Vec<String>, SplitError> {
    let words = split(s, Rules::Command)?;
    if words.is_empty() {
        return Err(SplitError { offset: 0, message: NO_WORDS });
    }
    Ok(words)
}

/// Splits the contents of a response file with the GNU rules
/// (`spec/cdb.md` §4): as [`split_command`], except that inside double
/// quotes a backslash is literal unless followed by `"` or `\`.
///
/// A file with no words splits to no words; it is not an error (§4 lists
/// the errors of a response file and an empty one is not among them).
pub fn split_response(s: &str) -> Result<Vec<String>, SplitError> {
    split(s, Rules::Gnu)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rules {
    /// §3.
    Command,
    /// §4: backslash inside double quotes escapes only `"` and `\`.
    Gnu,
}

/// §3: the word separators.
fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

fn split(s: &str, rules: Rules) -> Result<Vec<String>, SplitError> {
    let mut words = Vec::new();
    let mut word = String::new();
    // A word has started even if it is still empty (`""`, §3).
    let mut in_word = false;
    let mut chars = s.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            c if is_separator(c) => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\\' => {
                // §3: outside quotes a backslash makes the next character literal.
                let Some((_, next)) = chars.next() else {
                    return Err(SplitError { offset: at, message: TRAILING_BACKSLASH });
                };
                word.push(next);
                in_word = true;
            }
            '"' => {
                in_word = true;
                let unclosed = SplitError { offset: at, message: UNCLOSED_DOUBLE_QUOTE };
                loop {
                    let Some((_, q)) = chars.next() else {
                        return Err(unclosed);
                    };
                    match q {
                        '"' => break,
                        '\\' => {
                            let Some((_, next)) = chars.next() else {
                                return Err(unclosed);
                            };
                            match rules {
                                Rules::Command => word.push(next),
                                Rules::Gnu => {
                                    if next != '"' && next != '\\' {
                                        word.push('\\');
                                    }
                                    word.push(next);
                                }
                            }
                        }
                        other => word.push(other),
                    }
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    let Some((_, q)) = chars.next() else {
                        return Err(SplitError { offset: at, message: UNCLOSED_SINGLE_QUOTE });
                    };
                    if q == '\'' {
                        break;
                    }
                    word.push(q);
                }
            }
            other => {
                word.push(other);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}
