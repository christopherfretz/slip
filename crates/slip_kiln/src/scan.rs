//! The lexical half of the residue scan (`spec/edits.md` §8.4; work
//! package WP-068, `spec/landed/edits/WP-068.md`): where a file still
//! spells a name.
//!
//! **Kiln's lexer decides what a token is.** The file's bytes are lexed
//! by `kiln_lex::Lexer` in raw mode: one buffer, no preprocessing, no
//! directive obeyed, comments kept as tokens, with the options of the
//! latest C++ standard Kiln has (`LexOptions::new(Std::Cxx26)`). Every
//! token's extent and kind is the lexer's; an identifier's spelling is its
//! interned symbol, which resolves universal-character-names and line
//! splices (Kiln `TokKind::Identifier`). This module decides two things
//! only:
//!
//! - **where a header name is lexed**, as Kiln's preprocessor asks the
//!   lexer for one (`LexMode::HeaderName`): the token after `#include`,
//!   `#include_next`, `#import`, `#embed` at the start of a line, after a
//!   `(` that follows `__has_include`, `__has_include_next` or
//!   `__has_embed`, and after `import` (or `export import`) at the start
//!   of a line, when it is on the same line (a header name cannot follow
//!   on the next one: the lexer is told it is in a directive line, and
//!   the end of the line ends the wait). No hit is inside a header name;
//! - **what a mention is**: a whole word equal to the name inside the
//!   spelling (line splices removed, Kiln `kiln_lex::clean`) of a comment,
//!   a string literal, a raw string literal, a character literal, or an
//!   error-recovery token (an unterminated literal), mapped back to the
//!   bytes as written by Kiln's `kiln_lex::source_offset`.
//!
//! A miss is a silent residue, so where it has to choose the scan reports:
//! an ASCII letter or digit right after an odd run of `\` in a literal is
//! an escape (`"\nfoo"` mentions `foo`), not part of a word.
//!
//! **Bytes that are not UTF-8.** Kiln's lexer reads text, and the scan
//! has no file to load. Each byte of an invalid UTF-8 sequence is lexed
//! as U+0001, so the text has the file's length and every offset is the
//! file's. Kiln, loading the file, makes each maximal invalid sequence one
//! U+FFFD that its lexer knows for a replacement (`load_report`): it ends
//! an identifier and is dropped between tokens
//! (`kiln_lex/src/lexer.rs:574`, `:1060` at `913a490bb`). U+0001 does the
//! same for every hit: it is no identifier character, no whitespace (so a
//! `\` before it splices no line), no character of a raw string's
//! delimiter and no word character. A U+FFFD spelled in a virtual buffer
//! would not do: Kiln's lexer keeps it inside an identifier for recovery.

use crate::kiln::lex::{
    Interner, LexMode, LexOptions, Lexer, Punct, Std, Symbol, TokFlags, TokKind, Token, clean,
    is_identifier, source_offset,
};
use crate::kiln::source::SourceManager;

/// One place where a file spells a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Byte offsets in the file: an identifier token's bytes as written,
    /// or a mention's word.
    pub lo: u64,
    pub hi: u64,
    /// 1-based; lines end at `\n`, `\r\n` or a lone `\r` (Kiln's rule).
    pub line: u64,
    /// 1-based, in bytes of the file (a byte-order mark counts).
    pub column: u64,
    /// The name is a word inside a comment or literal, not an identifier.
    pub mention: bool,
}

/// Every identifier token of `bytes` spelled `name` (`mention` false) and
/// every comment, string literal or character literal that contains
/// `name` as a whole word (`mention` true; the range is the word's), in
/// ascending order of `lo` (module documentation).
///
/// `name` is the identifier's characters in UTF-8; an empty `name`, or
/// one that is not UTF-8, spells nothing. A file too large for Kiln's
/// source manager (its location space, about 2 GiB) gives no hit.
pub fn scan(bytes: &[u8], name: &[u8]) -> Vec<Hit> {
    let Ok(name) = std::str::from_utf8(name) else { return Vec::new() };
    if name.is_empty() {
        return Vec::new();
    }
    let text = text_of(bytes);
    let sm = SourceManager::new();
    let Ok(file) = sm.add_virtual("<scan>", text) else { return Vec::new() };
    let interner = Interner::new();
    let want = interner.intern(name);
    let words = Words::new(&interner);
    let mut opts = LexOptions::new(Std::Cxx26);
    opts.keep_comments = true;
    let base = sm.loc(file, 0);
    let mut lx = Lexer::new(&sm, &interner, file, base, opts);
    let mut header = HeaderState::default();
    let mut out = Vec::new();
    let mut hit = |lo: usize, hi: usize, mention: bool| {
        out.push(place(&sm, file, lo, hi, mention));
    };
    loop {
        if header.next_is_header_name() {
            lx.set_mode(LexMode::HeaderName);
            lx.set_in_directive(true);
        }
        let t = lx.next();
        if t.kind == TokKind::Eof {
            break;
        }
        // The line ended before a header name: none is lexed on the next
        // line (`import` newline `<foo>`, `#include` newline `"foo"`).
        if t.kind == TokKind::Eod {
            lx.set_mode(LexMode::Normal);
            header.line_ended();
            continue;
        }
        if t.kind != TokKind::Comment {
            lx.set_in_directive(false);
        }
        let Some(lo) = sm.same_entry_distance(base, t.span.lo) else { continue };
        let Some(hi) = sm.same_entry_distance(base, t.span.hi) else { continue };
        let (lo, hi) = (lo as usize, hi as usize);
        header.see(&t, &words);
        match t.kind {
            TokKind::Identifier(s) if s == want => hit(lo, hi, false),
            TokKind::Comment
            | TokKind::StringLit(_)
            | TokKind::CharLit(_)
            | TokKind::RawStringLit(_)
            | TokKind::Unknown => {
                let src = &sm.text(file)[lo..hi];
                for (a, b) in mentions(src, t.kind, name) {
                    hit(lo + a, lo + b, true);
                }
            }
            _ => {}
        }
    }
    out
}

/// The symbols the header-name rule looks at, in the scan's interner.
struct Words {
    directives: [Symbol; 4],
    has: [Symbol; 3],
    import: Symbol,
    export: Symbol,
}

impl Words {
    fn new(i: &Interner) -> Words {
        Words {
            directives: ["include", "include_next", "import", "embed"].map(|w| i.intern(w)),
            has: ["__has_include", "__has_include_next", "__has_embed"].map(|w| i.intern(w)),
            import: i.intern("import"),
            export: i.intern("export"),
        }
    }
}

/// Where the next token is a header name (module documentation).
#[derive(Default)]
struct HeaderState {
    /// A token other than a comment was lexed on this line.
    code_on_line: bool,
    /// The previous token other than a comment, and whether it was the
    /// first of its line.
    prev: Option<(TokKind, bool)>,
    /// The next token is to be lexed as a header name.
    header_next: bool,
}

impl HeaderState {
    fn next_is_header_name(&self) -> bool {
        self.header_next
    }

    /// The line ended where a header name was awaited.
    fn line_ended(&mut self) {
        self.header_next = false;
    }

    fn see(&mut self, t: &Token, w: &Words) {
        if t.has(TokFlags::START_OF_LINE) {
            self.code_on_line = false;
        }
        if t.kind == TokKind::Comment {
            return;
        }
        let first = !self.code_on_line;
        self.code_on_line = true;
        let ident = |k: Option<(TokKind, bool)>, set: &[Symbol]| matches!(k, Some((TokKind::Identifier(s), _)) if set.contains(&s));
        self.header_next = match t.kind {
            // `# include`, `%: include`.
            TokKind::Identifier(s) if w.directives.contains(&s) => {
                matches!(self.prev, Some((TokKind::Punct(Punct::Hash), true)))
                    || (s == w.import
                        && (first
                            || matches!(self.prev, Some((TokKind::Identifier(e), true)) if e == w.export)))
            }
            // `__has_include (`.
            TokKind::Punct(Punct::LParen) => ident(self.prev, &w.has),
            _ => false,
        };
        self.prev = Some((t.kind, first));
    }
}

/// The whole-word occurrences of `name` in the token whose source text
/// is `src`, as byte ranges of `src`. Occurrences may overlap (`"\nnn"`
/// with the name `nn`: the one at the escape's `n` is not a word, the
/// next one is), so every start is tried.
fn mentions(src: &str, kind: TokKind, name: &str) -> Vec<(usize, usize)> {
    let s = clean(src, kind, false);
    let literal = !matches!(kind, TokKind::Comment | TokKind::RawStringLit(_));
    let step = name.chars().next().map_or(1, char::len_utf8);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = s[from..].find(name) {
        let a = from + k;
        from = a + step;
        let b = a + name.len();
        let before = s[..a].chars().next_back();
        let after = s[b..].chars().next();
        let escape = literal && before.is_some_and(|c| c.is_ascii_alphanumeric()) && {
            let pre = &s.as_bytes()[..a - 1];
            pre.iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1
        };
        if (before.is_some_and(is_word_char) && !escape) || after.is_some_and(is_word_char) {
            continue;
        }
        // The word's first and last characters, where they are written.
        let last = name.chars().next_back().map_or(0, char::len_utf8);
        let lo = source_offset(src, kind, false, a);
        let hi = source_offset(src, kind, false, b - last) + last;
        out.push((lo, hi));
    }
    out
}

/// A character that continues an identifier: `$` (Kiln's default
/// `dollar_in_identifiers`) or what Kiln's `is_identifier` takes after
/// `_`.
fn is_word_char(c: char) -> bool {
    if c == '$' {
        return true;
    }
    let mut buf = [0u8; 5];
    buf[0] = b'_';
    let n = c.encode_utf8(&mut buf[1..]).len();
    std::str::from_utf8(&buf[..=n]).is_ok_and(is_identifier)
}

/// `bytes` as text of the same length: each byte of an invalid UTF-8
/// sequence becomes U+0001 (module documentation, "Bytes that are not
/// UTF-8").
fn text_of(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    let mut text = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        text.push_str(chunk.valid());
        text.extend(std::iter::repeat_n('\u{1}', chunk.invalid().len()));
    }
    text
}

/// The hit for offsets `lo..hi`, with Kiln's line and column
/// (`SourceManager::line_col`: lines end at `\n`, `\r\n` or a lone
/// `\r`; columns in bytes).
fn place(
    sm: &SourceManager,
    file: crate::kiln::source::FileId,
    lo: usize,
    hi: usize,
    mention: bool,
) -> Hit {
    let (line, column) =
        sm.line_col(sm.loc(file, lo as u32)).map_or((1, lo as u32 + 1), |c| (c.line, c.col));
    Hit { lo: lo as u64, hi: hi as u64, line: u64::from(line), column: u64::from(column), mention }
}
