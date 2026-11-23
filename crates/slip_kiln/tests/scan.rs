//! `slip_kiln::scan` (work package WP-068, `spec/landed/edits/WP-068.md`):
//! the lexical half of the residue scan, `spec/edits.md` §8.4. Every test
//! names the case of the package or of the brief it covers; each asserts
//! the exact hits (offsets, line, column, mention).

use slip_kiln::scan::{Hit, scan};

/// The hits as `(text of the range, line, column, mention)`, after
/// checking that ranges are in ascending order and lie in `src`.
fn hits(src: &[u8], name: &str) -> Vec<(String, u64, u64, bool)> {
    let hs = scan(src, name.as_bytes());
    assert!(hs.windows(2).all(|w| w[0].lo < w[1].lo), "ascending by lo: {hs:?}");
    hs.iter()
        .map(|h: &Hit| {
            assert!(h.lo < h.hi && h.hi as usize <= src.len(), "in bounds: {h:?}");
            let t = String::from_utf8_lossy(&src[h.lo as usize..h.hi as usize]).into_owned();
            (t, h.line, h.column, h.mention)
        })
        .collect()
}

fn id(t: &str, line: u64, column: u64) -> (String, u64, u64, bool) {
    (t.to_owned(), line, column, false)
}

fn mention(t: &str, line: u64, column: u64) -> (String, u64, u64, bool) {
    (t.to_owned(), line, column, true)
}

// §8.4: identifier tokens whose spelling is the name.
#[test]
fn s8_4_identifiers() {
    let src = b"int foo;\nvoid g() { foo = 1; ::foo; n::foo(); }\n";
    assert_eq!(
        hits(src, "foo"),
        vec![id("foo", 1, 5), id("foo", 2, 12), id("foo", 2, 23), id("foo", 2, 31)]
    );
}

// The name as a prefix and a suffix of a longer identifier: no hit.
#[test]
fn s8_4_prefix_and_suffix_are_no_hit() {
    assert_eq!(hits(b"int foobar, barfoo, foo_, _foo, foo1, foo;", "foo"), vec![id("foo", 1, 39)]);
}

// §8.4 mentions: a comment holding the name as a word, and as part of a
// word (no hit).
#[test]
fn s8_4_comment_word_and_part_of_a_word() {
    let src = b"// see foo, foo_bar and xfoo\n/* foo */ int x;\n// foo.\n";
    assert_eq!(
        hits(src, "foo"),
        vec![mention("foo", 1, 8), mention("foo", 2, 4), mention("foo", 3, 4)]
    );
}

// Mentions in string and character literals, with prefixes and a
// ud-suffix; an escape before the word does not join it.
#[test]
fn s8_4_string_and_char_literals() {
    let src = br#"auto a = "foo"; auto b = u8"a foo b"; auto c = 'foo'; auto d = "\nfoo\t"; auto e = "\\foo";"#;
    let h = hits(src, "foo");
    assert_eq!(
        h,
        vec![
            mention("foo", 1, 11),
            mention("foo", 1, 31),
            mention("foo", 1, 49),
            mention("foo", 1, 67),
            mention("foo", 1, 87)
        ]
    );
    // `"\\foo"`: the backslash is escaped, `\\` then `foo`: still a word
    // (a backslash is no identifier character).
    // A word joined to a letter that is not an escape is not a mention.
    assert_eq!(hits(br#"auto a = "xfoo"; auto b = "\\xfoo";"#, "foo"), vec![]);
}

// A raw string with a delimiter: a word of its body is a mention, and so
// is the delimiter, a word of the literal (the scan errs to reporting);
// where the literal ends is the lexer's (`)" foo"` is body).
#[test]
fn s8_4_raw_string_with_a_delimiter() {
    let src = b"auto s = R\"foo(a foo\n)\" foo\")foo\"; int foo;";
    assert_eq!(
        hits(src, "foo"),
        vec![
            mention("foo", 1, 12),
            mention("foo", 1, 18),
            mention("foo", 2, 4),
            mention("foo", 2, 9),
            id("foo", 2, 19)
        ]
    );
    assert_eq!(hits(b"auto s = R\"x(a foo)x\";", "foo"), vec![mention("foo", 1, 16)]);
}

// A line splice inside an identifier does not split it; the range is the
// bytes as written. A splice inside a comment's word is removed too.
#[test]
fn s8_4_line_splice_inside_an_identifier() {
    let src = b"int fo\\\no;\n// f\\\noo\n";
    assert_eq!(hits(src, "foo"), vec![id("fo\\\no", 1, 5), mention("f\\\noo", 3, 4)]);
    // CRLF splice.
    let src = b"int fo\\\r\no;\r\n";
    assert_eq!(hits(src, "foo"), vec![id("fo\\\r\no", 1, 5)]);
}

// A universal-character-name and the character it names are one
// identifier; the range is the bytes as written.
#[test]
fn s8_4_universal_character_name() {
    let src = "int caf\\u00e9; int café; int caf\\U000000E9; int caf\\u{e9};".as_bytes();
    assert_eq!(
        hits(src, "café"),
        vec![
            id("caf\\u00e9", 1, 5),
            id("café", 1, 20),
            id("caf\\U000000E9", 1, 31),
            id("caf\\u{e9}", 1, 50)
        ]
    );
}

// A `#include` header name holds no hit, by either spelling and after
// `%:`; a macro named in `#include` is an identifier.
#[test]
fn s8_4_include_paths() {
    let src = b"#include <foo>\n#include \"foo.h\"\n%:include <foo/foo.h>\n  #  include_next <foo>\n#include foo\n#import <foo>\n#embed <foo> limit(1)\n";
    assert_eq!(hits(src, "foo"), vec![id("foo", 5, 10)]);
    let src = b"#if __has_include(<foo>) && __has_include(\"foo\") && __has_embed(<foo>)\n#endif\nimport <foo>;\nexport import <foo>;\n";
    assert_eq!(hits(src, "foo"), vec![]);
    // A `<` that does not follow a directive or `__has_include(` is no
    // header name: `a <foo> b` has the identifier.
    assert_eq!(
        hits(b"bool x = a <foo> b;\nint include <foo>;", "foo"),
        vec![id("foo", 1, 13), id("foo", 2, 14)]
    );
}

// `#error` and `#warning` lines are raw-lexed: their identifiers are
// hits, and an apostrophe's unterminated literal still yields mentions.
#[test]
fn s8_4_error_lines() {
    let src = b"#error foo is gone\n#warning can't use foo here\n";
    assert_eq!(hits(src, "foo"), vec![id("foo", 1, 8), mention("foo", 2, 20)]);
}

// A skipped `#if 0` group is text like any other: raw lexing sees it.
#[test]
fn s8_4_text_in_skipped_groups() {
    let src = b"#if 0\nint foo;\n#else\nint bar;\n#endif\n#ifdef foo\n#endif\n";
    assert_eq!(hits(src, "foo"), vec![id("foo", 2, 5), id("foo", 6, 8)]);
}

// Stringized and pasted tokens: `#foo` names `foo`; `a##b` forms no
// mention of `ab` (a known limit, `spec/landed/edits/WP-068.md`).
#[test]
fn s8_4_stringized_and_pasted() {
    let src = b"#define S(x) #x\n#define P(a, b) a##b\nS(foo) P(fo, o) P(ab, c)\n";
    assert_eq!(hits(src, "foo"), vec![id("foo", 3, 3)]);
    assert_eq!(hits(src, "abc"), vec![]);
}

// Digraphs and `%:` directives.
#[test]
fn s8_4_digraphs() {
    let src = b"%:define foo <:1:>\nint a<:foo:> = <%foo%>;\n";
    assert_eq!(hits(src, "foo"), vec![id("foo", 1, 10), id("foo", 2, 8), id("foo", 2, 18)]);
}

// A file that ends inside a comment, a literal or a raw string.
#[test]
fn s8_4_unterminated_at_end_of_file() {
    assert_eq!(hits(b"int foo; /* foo and", "foo"), vec![id("foo", 1, 5), mention("foo", 1, 13)]);
    assert_eq!(hits(b"int x; // foo", "foo"), vec![mention("foo", 1, 11)]);
    assert_eq!(hits(b"auto s = \"a foo", "foo"), vec![mention("foo", 1, 13)]);
    assert_eq!(hits(b"auto c = 'foo", "foo"), vec![mention("foo", 1, 11)]);
    let raw = hits(b"auto r = R\"(a foo\nfoo", "foo");
    assert!(!raw.is_empty(), "an unterminated raw string reports: {raw:?}");
    assert!(raw.iter().all(|h| h.3 || h.0 == "foo"), "{raw:?}");
}

// An empty file, an empty name, a name that is not UTF-8.
#[test]
fn s8_4_empty() {
    assert_eq!(hits(b"", "foo"), vec![]);
    assert!(scan(b"int foo;", b"").is_empty());
    assert!(scan(b"int foo;", b"\xff").is_empty());
}

// Bytes that are not UTF-8 around a hit: offsets, lines and columns are
// the file's bytes, not Kiln's text (each replacement is three bytes
// there).
#[test]
fn s8_4_invalid_utf8_around_a_hit() {
    let src = b"// \xff\xfe\n\xffint foo; /* \xc3 foo \xe2\x82 */ int\xffbar foo\xff;\n";
    let h = scan(src, b"foo");
    let at: Vec<(u64, u64, u64, u64, bool)> =
        h.iter().map(|h| (h.lo, h.hi, h.line, h.column, h.mention)).collect();
    let pos = |from: usize| src[from..].windows(3).position(|w| w == b"foo").unwrap() + from;
    let (a, b, c) = (pos(0), pos(pos(0) + 1), pos(pos(pos(0) + 1) + 1));
    let line2 = 6;
    assert_eq!(
        at,
        vec![
            (a as u64, a as u64 + 3, 2, (a - line2 + 1) as u64, false),
            (b as u64, b as u64 + 3, 2, (b - line2 + 1) as u64, true),
            (c as u64, c as u64 + 3, 2, (c - line2 + 1) as u64, false),
        ]
    );
}

// CRLF line ends and a lone CR are line terminators; a byte-order mark
// is a byte of the first line (it counts in the column).
#[test]
fn s8_4_crlf_and_byte_order_mark() {
    assert_eq!(hits(b"int a;\r\nint foo;\r\n\rfoo", "foo"), vec![id("foo", 2, 5), id("foo", 4, 1)]);
    let src = b"\xef\xbb\xbfint foo;\n";
    let h = scan(src, b"foo");
    assert_eq!(h, vec![Hit { lo: 7, hi: 10, line: 1, column: 8, mention: false }]);
    assert_eq!(hits(b"\xef\xbb\xbffoo", "foo"), vec![id("foo", 1, 4)]);
}

// `'` as a digit separator is the lexer's: no character literal starts.
#[test]
fn s8_4_digit_separator() {
    assert_eq!(hits(b"int x = 1'000; int foo = 2'0; // '\n", "foo"), vec![id("foo", 1, 20)]);
}

// A name with `$` and one that is a keyword elsewhere are identifiers by
// the lexer's rules; `$` joins a word.
#[test]
fn s8_4_dollar() {
    assert_eq!(
        hits(b"int foo$; int foo; // foo$ $foo foo", "foo"),
        vec![id("foo", 1, 15), mention("foo", 1, 33)]
    );
}

// A comment before `#` or between the directive's name and the header
// name changes nothing: the header name holds no hit.
#[test]
fn s8_4_include_after_comments() {
    let src =
        b"/* c */ #include <foo>\n# /* c */ include /* c */ <foo>\n/* a\n */ #include <foo>\n";
    assert_eq!(hits(src, "foo"), vec![]);
    // A `#` that is not first on its line starts no directive.
    assert_eq!(hits(b"x # include <foo>\n", "foo"), vec![id("foo", 1, 14)]);
}

// An invalid byte between `\` and a newline: Kiln's replacement is no
// whitespace, so no line is spliced and no `foo` is formed.
#[test]
fn s8_4_invalid_byte_after_a_backslash_splices_nothing() {
    assert_eq!(hits(b"int fo\\\xff\no;", "foo"), vec![]);
    assert_eq!(hits(b"int fo\\ \no;", "foo"), vec![id("fo\\ \no", 1, 5)]);
}

// A string literal's ud-suffix is part of the literal: a mention.
#[test]
fn s8_4_ud_suffix() {
    assert_eq!(hits(b"auto a = \"x\"_foo; auto b = 12_foo;", "_foo"), vec![mention("_foo", 1, 13)]);
}
