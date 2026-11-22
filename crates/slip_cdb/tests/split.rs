//! `spec/cdb.md` §3 (splitting a command string) and the GNU variant of §4
//! (splitting a response file): one case per rule and per error.

use slip_cdb::split::{
    NO_WORDS, SplitError, TRAILING_BACKSLASH, UNCLOSED_DOUBLE_QUOTE, UNCLOSED_SINGLE_QUOTE,
    split_command, split_response,
};

fn cmd(s: &str) -> Vec<String> {
    split_command(s).unwrap_or_else(|e| panic!("split_command({s:?}) failed: {e}"))
}

fn rsp(s: &str) -> Vec<String> {
    split_response(s).unwrap_or_else(|e| panic!("split_response({s:?}) failed: {e}"))
}

fn err(offset: usize, message: &'static str) -> SplitError {
    SplitError { offset, message }
}

// §3 rule: words are separated by one or more of space, tab, newline, CR.
#[test]
fn s3_separators() {
    assert_eq!(cmd("a b"), ["a", "b"]);
    assert_eq!(cmd("a\tb"), ["a", "b"]);
    assert_eq!(cmd("a\nb"), ["a", "b"]);
    assert_eq!(cmd("a\rb"), ["a", "b"]);
    assert_eq!(cmd("  a \t\r\n  b  "), ["a", "b"]);
    assert_eq!(cmd("clang++ -c x.cpp"), ["clang++", "-c", "x.cpp"]);
}

// §3: other whitespace (form feed, vertical tab, non-breaking space) is not
// a separator.
#[test]
fn s3_only_four_separators() {
    assert_eq!(cmd("a\x0cb"), ["a\x0cb"]);
    assert_eq!(cmd("a\x0bb"), ["a\x0bb"]);
    assert_eq!(cmd("a\u{a0}b"), ["a\u{a0}b"]);
}

// §3 rule: outside quotes, a backslash makes the next character literal.
#[test]
fn s3_backslash_outside_quotes() {
    assert_eq!(cmd(r"a\ b"), ["a b"]);
    assert_eq!(cmd(r"\\"), [r"\"]);
    assert_eq!(cmd(r#"\""#), [r#"""#]);
    assert_eq!(cmd(r"\'"), ["'"]);
    assert_eq!(cmd(r"\x"), ["x"]);
    assert_eq!(cmd("a\\\nb"), ["a\nb"]);
    assert_eq!(cmd(r"-DX=\(1\)"), ["-DX=(1)"]);
}

// §3 rule: a double-quoted part runs to the next unescaped `"`; inside it a
// backslash makes the next character literal; every other character is itself.
#[test]
fn s3_double_quotes() {
    assert_eq!(cmd(r#""a b""#), ["a b"]);
    assert_eq!(cmd(r#""a\"b""#), [r#"a"b"#]);
    assert_eq!(cmd(r#""a\\b""#), [r"a\b"]);
    assert_eq!(cmd(r#""a\nb""#), ["anb"]);
    assert_eq!(cmd(r#""it's""#), ["it's"]);
    assert_eq!(cmd("\"a\tb\nc\""), ["a\tb\nc"]);
    // The CMake shape for a string-valued macro.
    assert_eq!(cmd(r#"-DV=\"18\""#), [r#"-DV="18""#]);
    assert_eq!(cmd(r#""-DV=\"18\"""#), [r#"-DV="18""#]);
}

// §3 rule: a single-quoted part runs to the next `'`; every character inside
// is itself, backslash included.
#[test]
fn s3_single_quotes() {
    assert_eq!(cmd("'a b'"), ["a b"]);
    assert_eq!(cmd(r"'a\b'"), [r"a\b"]);
    assert_eq!(cmd(r"'a\'"), [r"a\"]);
    assert_eq!(cmd(r#"'a"b'"#), [r#"a"b"#]);
}

// §3 rule: parts that touch form one word.
#[test]
fn s3_parts_join() {
    assert_eq!(cmd(r#"-D"A B"'C'"#), ["-DA BC"]);
    assert_eq!(cmd(r#"a"b"c'd'e"#), ["abcde"]);
    assert_eq!(cmd(r#""a"" b""#), ["a b"]);
}

// §3 rule: an empty quoted part makes an empty word.
#[test]
fn s3_empty_quoted_word() {
    assert_eq!(cmd(r#""""#), [""]);
    assert_eq!(cmd("''"), [""]);
    assert_eq!(cmd(r#"a "" b"#), ["a", "", "b"]);
    assert_eq!(cmd("a '' b"), ["a", "", "b"]);
}

// §3 error: a quote that is never closed (offset of the opening quote).
#[test]
fn s3_error_unclosed_quote() {
    assert_eq!(split_command(r#"a "b"#), Err(err(2, UNCLOSED_DOUBLE_QUOTE)));
    assert_eq!(split_command("a 'b"), Err(err(2, UNCLOSED_SINGLE_QUOTE)));
    assert_eq!(split_command(r#"a "b\""#), Err(err(2, UNCLOSED_DOUBLE_QUOTE)));
    assert_eq!(split_command(r#""b\"#), Err(err(0, UNCLOSED_DOUBLE_QUOTE)));
}

// §3 error: a backslash as the last character.
#[test]
fn s3_error_trailing_backslash() {
    assert_eq!(split_command(r"a b\"), Err(err(3, TRAILING_BACKSLASH)));
    assert_eq!(split_command(r"\"), Err(err(0, TRAILING_BACKSLASH)));
}

// §3 error: a string with no words.
#[test]
fn s3_error_no_words() {
    assert_eq!(split_command(""), Err(err(0, NO_WORDS)));
    assert_eq!(split_command(" \t\r\n "), Err(err(0, NO_WORDS)));
}

// §3: offsets are byte offsets.
#[test]
fn s3_error_offset_is_bytes() {
    assert_eq!(split_command("é \"x"), Err(err(3, UNCLOSED_DOUBLE_QUOTE)));
}

// §4 (GNU): inside double quotes a backslash is literal unless followed by
// `"` or `\`.
#[test]
fn s4_gnu_backslash_in_double_quotes() {
    assert_eq!(rsp(r#""a\nb""#), [r"a\nb"]);
    assert_eq!(rsp(r#""C:\dir\file""#), [r"C:\dir\file"]);
    assert_eq!(rsp(r#""a\"b""#), [r#"a"b"#]);
    assert_eq!(rsp(r#""a\\b""#), [r"a\b"]);
    assert_eq!(rsp(r#""a\\""#), [r"a\"]);
    assert_eq!(rsp(r#""\\\"""#), [r#"\""#]);
}

// §4 (GNU): every other rule is §3's.
#[test]
fn s4_gnu_other_rules_as_s3() {
    assert_eq!(rsp("a  b\n\tc\r\n"), ["a", "b", "c"]);
    assert_eq!(rsp(r"a\ b \x"), ["a b", "x"]);
    assert_eq!(rsp(r"'a\b c'"), [r"a\b c"]);
    assert_eq!(rsp(r#"-D"A B"'C'"#), ["-DA BC"]);
    assert_eq!(rsp(r#""" ''"#), ["", ""]);
}

// §4 (GNU) errors: malformed quoting.
#[test]
fn s4_gnu_errors() {
    assert_eq!(split_response(r#"a "b"#), Err(err(2, UNCLOSED_DOUBLE_QUOTE)));
    assert_eq!(split_response("'b"), Err(err(0, UNCLOSED_SINGLE_QUOTE)));
    assert_eq!(split_response(r"b\"), Err(err(1, TRAILING_BACKSLASH)));
    // A literal backslash before the end of a double-quoted part does not
    // close it.
    assert_eq!(split_response(r#""b\"#), Err(err(0, UNCLOSED_DOUBLE_QUOTE)));
}

// §4: an empty response file is not among §4's errors; it has no words.
#[test]
fn s4_gnu_empty_file_has_no_words() {
    assert_eq!(rsp(""), Vec::<String>::new());
    assert_eq!(rsp(" \n\t"), Vec::<String>::new());
}
