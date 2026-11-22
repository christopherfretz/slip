//! A small lexer of Rust source for the textual lints: identifiers and
//! punctuation with their lines, comments and literals left out.
//!
//! It is not a full Rust lexer. It knows what it must to skip text that is
//! not code: line and (nested) block comments, string literals (`"…"`,
//! `b"…"`, `c"…"`, raw `r#"…"#` in every form), character and byte
//! literals, and tells a character literal from a lifetime. Everything
//! else is an identifier (a run of `_` and alphanumerics, keywords and
//! numbers included) or one punctuation character, except `::`, which is
//! one token.

/// One token and its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'a> {
    pub text: &'a str,
    pub line: usize,
}

/// The tokens of `src`, in order.
pub fn tokens(src: &str) -> Vec<Token<'_>> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let ident = |c: u8| c == b'_' || c.is_ascii_alphanumeric() || c >= 0x80;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                while i < b.len() {
                    if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        i += 2;
                    } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if b[i] == b'\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                }
            }
            b'"' => i = skip_string(b, i + 1, &mut line),
            b'\'' => i = skip_quote(b, i, &mut line),
            b':' if b.get(i + 1) == Some(&b':') => {
                out.push(Token { text: &src[i..i + 2], line });
                i += 2;
            }
            c if ident(c) => {
                let start = i;
                while i < b.len() && ident(b[i]) {
                    i += 1;
                }
                let word = &src[start..i];
                // A literal prefix: `b"`, `c"`, `r"`, `r#"`, `br#"`, `b'`.
                let raw = matches!(word, "r" | "br" | "cr");
                let next = b.get(i).copied();
                if raw && matches!(next, Some(b'"' | b'#')) && raw_start(b, i) {
                    i = skip_raw(b, i, &mut line);
                } else if matches!(word, "b" | "c") && next == Some(b'"') {
                    i = skip_string(b, i + 1, &mut line);
                } else if word == "b" && next == Some(b'\'') {
                    i = skip_quote(b, i, &mut line);
                } else {
                    out.push(Token { text: word, line });
                }
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                // One character (a whole UTF-8 sequence).
                let len = src[i..].chars().next().map_or(1, char::len_utf8);
                out.push(Token { text: &src[i..i + len], line });
                i += len;
            }
        }
    }
    out
}

/// After the opening `"` at `i - 1`: the index after the closing one.
fn skip_string(b: &[u8], mut i: usize, line: &mut usize) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            b'\n' => {
                *line += 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    b.len()
}

/// Whether `#…#"` or `"` starts at `i` (a raw string after its `r`).
fn raw_start(b: &[u8], mut i: usize) -> bool {
    while b.get(i) == Some(&b'#') {
        i += 1;
    }
    b.get(i) == Some(&b'"')
}

/// At the `#`s or `"` after `r`: the index after the raw string.
fn skip_raw(b: &[u8], mut i: usize, line: &mut usize) -> usize {
    let mut hashes = 0;
    while b.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    i += 1; // the opening quote
    while i < b.len() {
        if b[i] == b'"' && b[i + 1..].iter().take(hashes).filter(|&&c| c == b'#').count() == hashes
        {
            return i + 1 + hashes;
        }
        if b[i] == b'\n' {
            *line += 1;
        }
        i += 1;
    }
    b.len()
}

/// At a `'`: a character literal is skipped whole; of a lifetime or label
/// (`'a`, `'static`) only the quote, the name then lexed as an identifier.
fn skip_quote(b: &[u8], i: usize, line: &mut usize) -> usize {
    match b.get(i + 1) {
        Some(b'\\') => {
            // `'\n'`, `'\''`, `'\u{…}'`: to the closing quote.
            let mut j = i + 3;
            while j < b.len() && b[j] != b'\'' && b[j] != b'\n' {
                j += 1;
            }
            j + 1
        }
        Some(&c) => {
            // One character, then a quote: a literal. Else a lifetime.
            let len = std::str::from_utf8(&b[i + 1..])
                .ok()
                .and_then(|s| s.chars().next())
                .map_or(1, char::len_utf8);
            if b.get(i + 1 + len) == Some(&b'\'') {
                if c == b'\n' {
                    *line += 1;
                }
                i + 2 + len
            } else {
                i + 1
            }
        }
        None => i + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str) -> Vec<&str> {
        tokens(src).into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn skips_comments_and_literals() {
        assert_eq!(texts("a::b // c::d\n/* e /* f */ g */ h"), ["a", "::", "b", "h"]);
        assert_eq!(
            texts(r#"x("a::b\" c", b"d", c"e", 'f', b'g', '\'', '"')"#),
            ["x", "(", ",", ",", ",", ",", ",", ",", ")"]
        );
        assert_eq!(texts("r#\"a \" b\"# r\"c\" br##\"d\"## y"), ["y"]);
        assert_eq!(
            texts("fn f<'a>(x: &'a str) -> &'static str"),
            [
                "fn", "f", "<", "a", ">", "(", "x", ":", "&", "a", "str", ")", "-", ">", "&",
                "static", "str"
            ]
        );
        assert_eq!(texts("r # x"), ["r", "#", "x"]);
    }

    #[test]
    fn lines() {
        let t = tokens("a\n/* \n */ \"\n\" b\n'\\n'\nc");
        let got: Vec<(&str, usize)> = t.iter().map(|t| (t.text, t.line)).collect();
        assert_eq!(got, [("a", 1), ("b", 4), ("c", 6)]);
    }
}
