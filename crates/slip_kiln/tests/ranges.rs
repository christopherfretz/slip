//! `slip_kiln::ranges` (work package WP-084, `spec/landed/api/WP-084.md`):
//! the file range of a token span and its tier (`spec/api.md` §5.4,
//! `spec/edits.md` §2.2), and unit order (`spec/api.md` §1). Tests 1 to 6
//! of `docs/wp/WP-084.md`, each named after its number.
//!
//! The snippets are parsed by `snippet::parse_with_tables` and
//! preprocessed again by `snippet::preprocess`, which builds the source
//! manager as the parse does: the tests find tokens in the preprocessor's
//! token sequence and ask about their locations in the parse's unit
//! ([`with_unit`] checks that the two agree).
//!
//! Test 4, the property that defines correctness, is [`property`]: for
//! every range given, the file is preprocessed again with the bytes of
//! the range replaced by a marker, and exactly the span's tokens (tier
//! 0), or exactly the uses of the argument (tier 1), must have become
//! the marker. It runs over the snippets here and over every snippet of
//! the digest's pair tests (`s5_4_test4_property_over_digest_pair_tests`).

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::ids::ExprId;
use slip_kiln::kiln::lex::Token;
use slip_kiln::kiln::source::{ExpansionKind, FileId, Loc, SourceManager, Span};
use slip_kiln::ranges::{FileRange, Tier, file_range, file_range_of_span, unit_order};
use slip_kiln::snippet::{self, Extra, Parsed, Snippet};

// ---------------------------------------------------------------------
// Harness: a unit and its token sequence.

/// One translation unit in memory and its command line.
#[derive(Clone, Debug)]
struct Source {
    files: Vec<(String, String)>,
    main: String,
    include: Vec<String>,
    std: String,
    args: Vec<String>,
    target: Option<String>,
}

impl Source {
    /// `t.cpp` holding `main`, with `headers` beside it, as C++20.
    fn new(main: &str, headers: &[(&str, &str)]) -> Source {
        let mut files = vec![("t.cpp".to_owned(), main.to_owned())];
        files.extend(headers.iter().map(|(p, t)| ((*p).to_owned(), (*t).to_owned())));
        Source {
            files,
            main: "t.cpp".to_owned(),
            include: vec![String::new()],
            std: "c++20".to_owned(),
            args: Vec::new(),
            target: None,
        }
    }

    /// This source with the bytes `lo..hi` of `path` replaced by `with`.
    fn replaced(&self, path: &str, lo: usize, hi: usize, with: &str) -> Source {
        let mut s = self.clone();
        for (p, t) in &mut s.files {
            if p == path {
                t.replace_range(lo..hi, with);
            }
        }
        s
    }

    /// Runs `f` with the snippet and the extra arguments of this source.
    fn with<R>(&self, f: impl FnOnce(&Snippet<'_>, &Extra<'_>) -> R) -> R {
        let files: Vec<(&str, &str)> =
            self.files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        let include: Vec<&str> = self.include.iter().map(String::as_str).collect();
        let snippet =
            Snippet { files: &files, main: &self.main, include: &include, std: &self.std };
        let args: Vec<&str> = self.args.iter().map(String::as_str).collect();
        let extra = Extra { args: &args, target: self.target.as_deref() };
        f(&snippet, &extra)
    }

    /// The token sequence after phase 4.
    fn spellings(&self) -> Result<Seq, String> {
        self.with(|s, e| {
            snippet::preprocess(s, e, |pre| {
                Seq::of(pre.sm, pre.tokens, |f| pre.path(f).map(str::to_owned), pre.errors)
            })
        })
    }
}

/// Which entry of a file produced a token: the snippet path of the file
/// whose bytes produced it ([`producer_loc`]) and, outward, the path and
/// line of each `#include` that entered it. Two entries of one file (a
/// header without a guard, an X-macro `.def`) differ in the `#include`
/// lines, and a marker moves no line ([`marker`]), so an entry is the
/// same before and after a marking.
type Entry = std::sync::Arc<(String, Vec<(String, u32)>)>;

/// A token sequence after phase 4: each token's spelling and entry, and
/// the errors the preprocessor reported.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seq {
    text: Vec<String>,
    entry: Vec<Entry>,
    errors: u32,
}

impl Seq {
    fn of(
        sm: &SourceManager,
        toks: &[Token],
        path: impl Fn(FileId) -> Option<String>,
        errors: u32,
    ) -> Seq {
        let name = |f: Option<FileId>| f.and_then(&path).unwrap_or_else(|| "<?>".to_owned());
        let mut cache: HashMap<(Option<FileId>, Loc), Entry> = HashMap::new();
        let mut entry = Vec::with_capacity(toks.len());
        for t in toks {
            let at = producer_loc(sm, t.span.lo);
            let file = at.and_then(|l| sm.offset_of(l)).map(|(f, _)| f);
            let inc = at.map_or(Loc::INVALID, |l| sm.include_loc(l));
            let e = cache.entry((file, inc)).or_insert_with(|| {
                let mut chain = Vec::new();
                let mut l = inc;
                while l.is_valid() && chain.len() < 10_000 {
                    let line = sm.line_col(l).map_or(0, |c| c.line);
                    chain.push((name(sm.offset_of(l).map(|(f, _)| f)), line));
                    l = sm.include_loc(l);
                }
                std::sync::Arc::new((name(file), chain))
            });
            entry.push(e.clone());
        }
        Seq { text: toks.iter().map(|t| spelling(sm, t)).collect(), entry, errors }
    }
}

/// The text a token is spelled with, or `<?>`.
fn spelling(sm: &SourceManager, t: &Token) -> String {
    sm.span_text(t.span).unwrap_or("<?>").to_owned()
}

/// Whether the token `last..end` begins with `>` and is longer than one
/// byte: `file_range` refuses it (decision 6, interim).
fn may_be_split(sm: &SourceManager, last: Loc, end: Loc) -> bool {
    sm.span_text(Span::new(last, end)).is_some_and(|x| x.len() > 1 && x.starts_with('>'))
}

/// What a test sees: the parsed unit, its token sequence and the snippet
/// path of each buffer.
struct Seen<'a, 'r> {
    p: &'a Parsed<'r>,
    toks: &'a [Token],
    paths: &'a HashMap<FileId, String>,
    /// The errors the preprocessor reported.
    pp_errors: u32,
}

impl Seen<'_, '_> {
    fn sm(&self) -> &SourceManager {
        self.p.sm
    }

    /// The index of the `n`th (0-based) token spelled `text`.
    #[track_caller]
    fn nth(&self, text: &str, n: usize) -> usize {
        self.toks
            .iter()
            .enumerate()
            .filter(|(_, t)| self.sm().span_text(t.span) == Some(text))
            .nth(n)
            .unwrap_or_else(|| panic!("no token #{n} spelled {text:?}"))
            .0
    }

    /// `file_range` of tokens `i..=j`, checked against
    /// `file_range_of_span` of the same tokens (test 6), or against `None`
    /// when the last token may be a split `>` ([`may_be_split`]).
    fn range(&self, i: usize, j: usize) -> Option<FileRange> {
        let (first, last) = (self.toks[i].span.lo, self.toks[j].span.lo);
        let r = file_range(self.p.unit(), first, last);
        let span = Span::new(first, self.toks[j].span.hi);
        let s = if may_be_split(self.sm(), last, span.hi) {
            None
        } else {
            file_range_of_span(self.sm(), span)
        };
        assert_eq!(r, s, "test 6: token_end against the token's own end, tokens {i}..={j}");
        r
    }

    /// The text of `r`, as `path:text` outside `t.cpp`.
    fn text(&self, r: FileRange) -> String {
        let text = &self.sm().text(r.file)[r.lo as usize..r.hi as usize];
        match self.paths.get(&r.file).map(String::as_str) {
            Some("t.cpp") => text.to_owned(),
            Some(p) => format!("{p}:{text}"),
            None => format!("<buffer {}>:{text}", self.sm().file_name(r.file)),
        }
    }

    /// The text and tier of tokens `i..=j`, `None` if they have no range.
    fn edit(&self, i: usize, j: usize) -> Option<(String, Tier)> {
        self.range(i, j).map(|r| (self.text(r), r.tier))
    }

    /// [`Self::edit`] of tokens given as (spelling, occurrence) pairs.
    #[track_caller]
    fn edit_of(&self, first: (&str, usize), last: (&str, usize)) -> Option<(String, Tier)> {
        self.edit(self.nth(first.0, first.1), self.nth(last.0, last.1))
    }
}

/// Parses `src` (with Kiln's tables) and preprocesses it again, and runs
/// `f` on what both give. Err when the front end fails, or when the two
/// source managers disagree (a token whose text differs, a different
/// number of expansion entries or buffers): then the locations of one are
/// not the other's.
fn with_unit<R>(src: &Source, f: impl FnOnce(&Seen<'_, '_>) -> R) -> Result<R, String> {
    src.with(|s, e| {
        snippet::parse_with_tables(s, e, |p| {
            snippet::preprocess(s, e, |pre| {
                if p.sm.expansion_count() != pre.sm.expansion_count() {
                    return Err(format!(
                        "expansion entries: parse {}, preprocessing {}",
                        p.sm.expansion_count(),
                        pre.sm.expansion_count()
                    ));
                }
                if p.sm.file_ids().count() != pre.sm.file_ids().count() {
                    return Err("the two source managers have different buffers".to_owned());
                }
                if let Some(t) =
                    pre.tokens.iter().find(|t| p.sm.span_text(t.span) != pre.sm.span_text(t.span))
                {
                    return Err(format!("the token {:?} differs", pre.sm.span_text(t.span)));
                }
                let paths: HashMap<FileId, String> = pre
                    .sm
                    .file_ids()
                    .filter_map(|f| pre.path(f).map(|s| (f, s.to_owned())))
                    .collect();
                Ok(f(&Seen { p, toks: pre.tokens, paths: &paths, pp_errors: pre.errors }))
            })
        })
    })
    .and_then(|r| r)
    .and_then(|r| r)
}

/// [`with_unit`] of a `t.cpp`, which must parse.
#[track_caller]
fn unit<R>(main: &str, headers: &[(&str, &str)], f: impl FnOnce(&Seen<'_, '_>) -> R) -> R {
    with_unit(&Source::new(main, headers), f).unwrap_or_else(|e| panic!("{e}"))
}

#[allow(clippy::unnecessary_wraps, reason = "compared with `edit_of`'s `Option`")]
fn some(text: &str, tier: Tier) -> Option<(String, Tier)> {
    Some((text.to_owned(), tier))
}

use Tier::{File as T0, MacroArgument as T1};

// ---------------------------------------------------------------------
// Test 1: tier 0.

/// Test 1, `spec/api.md` §5.4 tier 0: plain text is itself.
#[test]
fn s5_4_test1_tier0_plain_text() {
    unit("int b, c;\nint a = b + c;\n", &[], |u| {
        assert_eq!(u.edit_of(("b", 1), ("c", 1)), some("b + c", T0));
        assert_eq!(u.edit_of(("a", 0), ("a", 0)), some("a", T0));
        assert_eq!(u.edit_of(("int", 1), (";", 1)), some("int a = b + c;", T0));
    });
}

/// Test 1, `spec/api.md` §5.4 rule 2: a span holding a whole object-like
/// and a whole function-like invocation; a span that is exactly one
/// invocation.
#[test]
fn s5_4_test1_tier0_whole_invocations() {
    let src = "#define ONE 1\n#define F(x) (x * 2)\nint v = 2 + ONE + F(3) + 4;\n";
    unit(src, &[], |u| {
        assert_eq!(u.edit_of(("2", 0), ("4", 0)), some("2 + ONE + F(3) + 4", T0));
        // F's expansion is `( 3 * 2 )`: from its `(` to its `)`.
        assert_eq!(u.edit_of(("(", 0), (")", 0)), some("F(3)", T0));
        assert_eq!(u.edit_of(("1", 0), ("1", 0)), some("ONE", T0));
        assert_eq!(u.edit_of(("1", 0), ("(", 0)), None, "ONE then F's first token only");
    });
}

/// Test 1, `spec/api.md` §5.4 and `docs/wp/WP-084.md` item 1: the one
/// token of `ID(foo)` is the whole expansion, so its range is the
/// invocation, at tier 0, as in clang.
#[test]
fn s5_4_test1_tier0_identity_macro_is_the_invocation() {
    unit("#define ID(x) x\nint foo;\nint y = ID(foo);\n", &[], |u| {
        let i = u.nth("foo", 1);
        assert!(u.toks[i].span.lo.is_macro(), "the token is an argument's");
        assert_eq!(u.edit(i, i), some("ID(foo)", T0));
    });
}

/// `nth` must not panic on a missing token silently: a guard for the
/// harness itself.
#[test]
#[should_panic(expected = "no token #0 spelled \"ONE\"")]
fn harness_nth_panics_on_a_token_that_is_not_there() {
    unit("#define ONE 1\nint v = ONE;\n", &[], |u| {
        u.nth("ONE", 0);
    });
}

// ---------------------------------------------------------------------
// Test 2: tier 1.

/// Test 2, `spec/api.md` §5.4 tier 1: a range inside one argument is
/// where the argument is written.
#[test]
fn s5_4_test2_tier1_inside_one_argument() {
    let src = "#define F(x) (x)\nint a, b;\nint v = F(a + b) + 1;\n";
    unit(src, &[], |u| {
        assert_eq!(u.edit_of(("a", 1), ("b", 1)), some("a + b", T1));
        assert_eq!(u.edit_of(("a", 1), ("a", 1)), some("a", T1));
        assert_eq!(u.edit_of(("+", 0), ("+", 0)), some("+", T1));
        // The whole expansion is the invocation, at tier 0.
        assert_eq!(u.edit_of(("(", 0), (")", 0)), some("F(a + b)", T0));
    });
}

/// Test 2, `spec/api.md` §5.4 rule 3: the same argument used twice gives
/// the same range for both uses.
#[test]
fn s5_4_test2_tier1_argument_used_twice() {
    let src = "#define T(x) x + x\nint v;\nint w = T(v) * 2;\n";
    unit(src, &[], |u| {
        let (one, two) = (u.nth("v", 1), u.nth("v", 2));
        let (r1, r2) = (u.range(one, one).unwrap(), u.range(two, two).unwrap());
        assert_eq!(r1, r2);
        assert_eq!((u.text(r1), r1.tier), ("v".to_owned(), T1));
        // From one use to the other: the whole expansion.
        assert_eq!(u.edit(one, two), some("T(v)", T0));
    });
}

/// Test 2: nested invocations, an argument passed on to another macro,
/// and an argument inside another argument (`docs/requests/KF3-location-
/// utilities.md` §3 edge rows 2 and 3).
#[test]
fn s5_4_test2_tier1_nested_invocations() {
    let src = "#define F(x) (x)\n#define G(y) F(y) + 0\nint k, m;\n\
               int v = G(k * 3);\nint w = F(F(m) * 4);\n";
    unit(src, &[], |u| {
        assert_eq!(u.edit_of(("k", 1), ("3", 0)), some("k * 3", T1), "passed on");
        assert_eq!(u.edit_of(("k", 1), ("k", 1)), some("k", T1));
        assert_eq!(u.edit_of(("m", 1), ("m", 1)), some("m", T1), "an argument in an argument");
        // `F(m)` is a whole invocation inside the outer argument.
        let open = u.nth("(", 2);
        let close = u.nth(")", 1);
        assert_eq!(u.edit(open, close), some("F(m)", T1));
    });
}

// ---------------------------------------------------------------------
// Test 3: none.

/// Test 3, `spec/api.md` §5.4 "higher": tokens of a replacement list that
/// are not the whole expansion, a span from file text into an expansion.
#[test]
fn s5_4_test3_none_replacement_list() {
    let src = "#define P q + r\nint q, r, s;\nint z = 1 + P * s;\n";
    unit(src, &[], |u| {
        assert_eq!(u.edit_of(("q", 1), ("q", 1)), None, "the first token of a replacement list");
        assert_eq!(u.edit_of(("r", 1), ("r", 1)), None, "the last token of one");
        assert_eq!(u.edit_of(("1", 0), ("q", 1)), None, "from file text into an expansion");
        assert_eq!(u.edit_of(("r", 1), ("s", 1)), None, "from an expansion into file text");
        // The whole expansion is the invocation.
        assert_eq!(u.edit_of(("q", 1), ("r", 1)), some("P", T0));
        assert_eq!(u.edit_of(("1", 0), ("r", 1)), some("1 + P", T0));
    });
}

/// Test 3: tokens from two different arguments.
#[test]
fn s5_4_test3_none_two_arguments() {
    let src = "#define G2(p, q) (p + q)\nint a, b;\nint v = G2(a, b);\n";
    unit(src, &[], |u| {
        assert_eq!(u.edit_of(("a", 1), ("b", 1)), None);
        assert_eq!(u.edit_of(("a", 1), ("a", 1)), some("a", T1));
    });
}

/// Test 3: a pasted token and a stringised argument (`spec/edits.md`
/// §2.2 tier 3). Kiln gives the invocation when the token is the whole
/// expansion (`S(abc)`), and nothing otherwise.
#[test]
fn s5_4_test3_none_pasted_and_stringised() {
    let src = "#define CAT(a) a ## _s\n#define CAT2(a, b) (a ## b)\n\
               #define S(x) #x\n#define S2(x) (#x)\n\
               int foo_s, xy;\nint v = CAT(foo) + CAT2(x, y);\n\
               const char* s = S(abc);\nconst char* t = S2(def);\n";
    unit(src, &[], |u| {
        let pasted = u.nth("foo_s", 1);
        assert_eq!(u.edit(pasted, pasted), None, "pasted with a file token");
        let xy = u.nth("xy", 1);
        assert_eq!(u.edit(xy, xy), None, "a pasted token inside an expansion");
        let abc = u.nth("\"abc\"", 0);
        assert_eq!(u.edit(abc, abc), some("S(abc)", T0), "the whole expansion");
        let def = u.nth("\"def\"", 0);
        assert_eq!(u.edit(def, def), None, "a stringised argument");
    });
}

/// Test 3: a span across two files; an empty span; a location where no
/// token starts.
#[test]
fn s5_4_test3_none_two_files_empty_and_not_a_token() {
    let h = "int h1 = 1;\n";
    unit("#include \"h.h\"\nint m = 2;\n", &[("h.h", h)], |u| {
        assert_eq!(u.edit_of(("h1", 0), ("h1", 0)), some("h.h:h1", T0));
        assert_eq!(u.edit_of(("1", 0), ("m", 0)), None, "across two files");
        let m = u.toks[u.nth("m", 0)].span;
        assert_eq!(file_range_of_span(u.sm(), Span::empty(m.lo)), None, "an empty span");
        assert_eq!(file_range_of_span(u.sm(), Span::new(m.hi, m.lo)), None, "end before begin");
        // `last` at the blank after `m`: no token starts there.
        assert_eq!(file_range(u.p.unit(), m.lo, m.hi), None);
        assert_eq!(file_range(u.p.unit(), Loc::INVALID, Loc::INVALID), None);
    });
}

/// Test 1 where raw location order is not text order: a span from a
/// macro token to a later file token (`P 2`), and one from an argument's
/// token to file text after the invocation (`ID(foo)(a)`). The tokens of
/// an expansion have raw locations above every file location, so an
/// empty check done on raw locations refuses both.
#[test]
fn s5_4_test1_tier0_from_an_expansion_into_file_text() {
    let src =
        "#define P 1 +\n#define ID(x) x\nint foo(int); int a;\nint v = P 2;\nint w = ID(foo)(a);\n";
    unit(src, &[], |u| {
        let (one, two) = (u.nth("1", 0), u.nth("2", 0));
        assert!(u.toks[one].span.lo.is_macro() && !u.toks[two].span.lo.is_macro());
        assert!(u.toks[one].span.lo > u.toks[two].span.lo, "raw order is not text order");
        assert_eq!(u.edit(one, two), some("P 2", T0));
        assert_eq!(u.edit_of(("foo", 1), (")", 1)), some("ID(foo)(a)", T0));
    });
}

/// The `(kind, begin, end)` of every expression of `u` whose begin is
/// spelled with `prefix`.
fn exprs_at(u: &Seen<'_, '_>, prefix: &str) -> Vec<(ExprKind, ExprId, Loc, Loc)> {
    let ast = u.p.ast;
    let n = u32::try_from(prefix.len()).unwrap();
    (1..=ast.exprs.len())
        .filter_map(|k| ExprId::try_new(u32::try_from(k).ok()?))
        .map(|e| (ast.exprs.get(e).kind(), e, ast.begin_loc(e), ast.end_loc(e)))
        .filter(|&(_, _, f, _)| {
            f.is_valid() && u.sm().span_text(Span::new(f, f.with_offset(n))) == Some(prefix)
        })
        .collect()
}

/// Decision 6 (`spec/landed/api/WP-084.md`): an AST node may begin inside
/// a token. The call of a user-defined literal, its callee, and an
/// operator expression it begins, begin at the literal's suffix; they get
/// no range, and neither does a span ending at a suffix.
#[test]
fn s5_4_test3_none_an_end_inside_a_token() {
    let src = "unsigned long long operator\"\"_k(unsigned long long);\n\
               const char* operator\"\"_s(const char*, decltype(sizeof 0));\n\
               auto a = 12_k + 1;\nauto b = \"xy\"_s;\n";
    unit(src, &[], |u| {
        let unit = u.p.unit();
        let mut suffixes = Vec::new();
        for lit in ["12_k", "\"xy\"_s"] {
            let t = u.toks[u.nth(lit, 0)].span;
            let suffix = t.lo.with_offset(u32::try_from(lit.len()).unwrap() - 2);
            assert!(unit.is_token_start(t.lo) && !unit.is_token_start(suffix), "{lit}");
            assert_eq!(file_range(unit, suffix, suffix), None, "{lit}: at the suffix");
            assert_eq!(file_range(unit, t.lo, suffix), None, "{lit}: last at the suffix");
            suffixes.push(suffix);
        }
        let one = u.toks[u.nth("1", 0)].span.lo;
        assert_eq!(file_range(unit, suffixes[0], one), None, "`_k + 1`");
        assert_eq!(u.edit_of(("12_k", 0), ("1", 0)), some("12_k + 1", T0), "from the token");
        let at_suffix = [exprs_at(u, "_k"), exprs_at(u, "_s")].concat();
        assert!(at_suffix.len() >= 3, "the calls, the callees, `+`: {at_suffix:?}");
        for (kind, _, f, l) in at_suffix {
            assert!(!unit.is_token_start(f));
            assert_eq!(file_range(unit, f, l), None, "{kind:?}");
        }
    });
}

/// Decision 3 (`spec/landed/api/WP-084.md`): shapes that §5.4's table
/// would give a whole-expansion range, where Kiln's rules do not see the
/// whole expansion, get less (review F7). Pinned, so that a change in
/// Kiln's rules shows here.
#[test]
fn s5_4_shapes_kiln_gives_less_than_the_table() {
    let src = "#define F(x, y) x y\n#define E(...) 2 __VA_OPT__(+ __VA_ARGS__)\n\
               #define PR _Pragma(\"slip_wp084\") a\nint a, b;\n\
               int v1 = F(, b);\nint v2 = F(b, );\nint v3 = E();\nint v4 = PR;\n";
    unit(src, &[], |u| {
        assert_eq!(u.edit_of(("b", 1), ("b", 1)), some("b", T1), "F(, b)");
        assert_eq!(u.edit_of(("b", 2), ("b", 2)), some("b", T1), "F(b, )");
        assert_eq!(u.edit_of(("2", 0), ("2", 0)), None, "E()");
        assert_eq!(u.edit_of(("a", 1), ("a", 1)), None, "PR");
    });
}

/// The source of review F1: `B<vt<1>> y;` ends `vt<1>` at the first `>`
/// of `>>`, which the parser split.
const SPLIT_GREATER: &str = "template <int N> constexpr int vt = N;\n\
     template <int N> struct B { static constexpr int c = N; };\n\
     B<vt<1>> y;\nint z = B<vt<B<1>::c>>::c;\n\
     struct Q {};\nint operator>>(Q, int);\nint s = operator>>(Q{}, 4 >> 1);\n";

/// Decision 6, interim (`spec/landed/api/WP-084.md` "Until Kiln
/// answers"): a span whose last token, as `token_end` lexes it, begins
/// with `>` and is longer than one byte gets no range, whether or not the
/// parser split it. The loss: a name ending in `>>`, `>=` or `>>=`
/// (`operator>>`), and a split `>` that ends a template argument list.
#[test]
fn s5_4_test3_none_a_possibly_split_greater() {
    unit(SPLIT_GREATER, &[], |u| {
        let vt: Vec<_> =
            exprs_at(u, "vt").into_iter().filter(|e| e.0 == ExprKind::DeclRefExpr).collect();
        assert_eq!(vt.len(), 2, "{vt:?}");
        for (_, _, f, l) in vt {
            assert_eq!(u.sm().span_text(Span::new(l, l.with_offset(2))), Some(">>"));
            assert_eq!(file_range(u.p.unit(), f, l), None, "split: no range");
        }
        assert_eq!(u.edit_of(("vt", 1), (">>", 0)), None, "the tokens as lexed");
        assert_eq!(
            u.edit_of(("operator", 1), (">>", 3)),
            None,
            "`operator>>` as written: the loss"
        );
        assert_eq!(u.edit_of(("4", 0), ("1", 2)), some("4 >> 1", T0), "a `>>` inside the span");
    });
}

/// The right answer for review F1, pending Kiln's record of split `>`
/// tokens: the range of `vt<1>` is `vt<1>`.
#[test]
#[ignore = "pending Kiln: no record of a split `>` (spec/landed/api/WP-084.md \"Until Kiln answers\")"]
fn s5_4_pending_a_split_greater_ends_at_its_first_byte() {
    unit(SPLIT_GREATER, &[], |u| {
        let vt: Vec<_> =
            exprs_at(u, "vt").into_iter().filter(|e| e.0 == ExprKind::DeclRefExpr).collect();
        let texts: Vec<_> = vt
            .iter()
            .map(|&(_, _, f, l)| file_range(u.p.unit(), f, l).map(|r| (u.text(r), r.tier)))
            .collect();
        assert_eq!(texts, [some("vt<1>", T0), some("vt<B<1>::c>", T0)]);
    });
}

/// Decision 10, interim (`spec/landed/api/WP-084.md` "Until Kiln
/// answers"): a range that holds a preprocessing directive, or a line of
/// a skipped group, is no range, at either tier. Review F3's two sources,
/// `#include`, `#pragma`, `#define`, `%:` and a directive after a comment
/// inside; a range inside one branch of an `#if`, and one over lines
/// with no directive, have a range.
#[test]
fn s5_4_test3_none_a_range_holding_a_directive() {
    let skipped = "int f(int, int); int b, c;\nint v = f(1,\n#ifdef X\n b\n#else\n c\n#endif\n);\n";
    unit(skipped, &[], |u| {
        assert_eq!(u.edit_of(("f", 1), (")", 1)), None, "the call holds the skipped `b`");
        assert_eq!(u.edit_of(("1", 0), ("c", 1)), None);
        assert_eq!(u.edit_of(("c", 1), ("c", 1)), some("c", T0), "inside one branch");
    });
    unit("int v = 1\n#if 1\n+ 2\n#endif\n;\n", &[], |u| {
        assert_eq!(u.edit_of(("1", 0), ("2", 0)), None, "`#if` without its `#endif`");
        assert_eq!(u.edit_of(("+", 0), ("2", 0)), some("+ 2", T0), "inside the group");
    });
    let cases = [
        ("#include \"e.h\"", "an #include"),
        ("#pragma slip_wp084", "a #pragma"),
        ("#define Z 0", "a #define"),
        ("%:define Y 0", "a digraph directive"),
        ("  /* c */ # define W 0", "a directive after blanks and a comment"),
    ];
    for (line, what) in cases {
        let src = format!("int v = (1\n{line}\n+ 2);\n");
        unit(&src, &[("e.h", "// nothing\n")], |u| {
            assert_eq!(u.edit_of(("(", 0), (")", 0)), None, "{what}");
            assert_eq!(u.edit_of(("+", 0), ("2", 0)), some("+ 2", T0), "{what}: after it");
        });
    }
    let multi = "#if 1\nint v = (1 +\n  2 *\n  3);\n#endif\n";
    unit(multi, &[], |u| {
        assert_eq!(u.edit_of(("(", 0), (")", 0)), some("(1 +\n  2 *\n  3)", T0), "no directive");
    });
    // Tier 1: an argument holding a conditional.
    let arg = "#define F(x) (x)\nint v = F(1\n#if 1\n+ 2\n#endif\n);\n";
    unit(arg, &[], |u| {
        assert_eq!(u.edit_of(("1", 0), ("2", 0)), None, "an argument holding `#if`");
        assert_eq!(u.edit_of(("1", 0), ("1", 0)), some("1", T1));
    });
    // The interim check reads bytes: a raw string with a line starting
    // with `#` is refused too (a loss until Kiln records directives).
    unit("const char* r = R\"(\n#x)\";\n", &[], |u| {
        let t = u.nth("R\"(\n#x)\"", 0);
        assert_eq!(u.edit(t, t), None, "interim: a raw string line beginning with #");
    });
}

/// What a consumer gets for an implicit node (review F5): its begin and
/// end are those of what it stands for, so `file_range` gives that
/// range. No range for an implicit node is the consumer's rule
/// (`spec/landed/api/WP-084.md`, "What a consumer must not assume").
#[test]
fn s5_4_implicit_nodes_have_the_range_of_what_they_stand_for() {
    let src = "struct S { int m; int g() { return m; } };\nint arr[2];\n\
               void h() { for (int x : arr) { (void)x; } }\n";
    unit(src, &[], |u| {
        let ast = u.p.ast;
        let this: Vec<_> = exprs_at(u, "m")
            .into_iter()
            .filter(|&(k, e, _, _)| k == ExprKind::CXXThisExpr && ast.exprs.this_is_implicit(e))
            .collect();
        assert_eq!(this.len(), 1, "{this:?}");
        let (_, _, f, l) = this[0];
        assert_eq!(file_range(u.p.unit(), f, l).map(|r| (u.text(r), r.tier)), some("m", T0));
        // The range-for's implicit variables and calls begin at `:`.
        let colon: Vec<_> = exprs_at(u, ":")
            .into_iter()
            .filter_map(|(_, _, f, l)| file_range(u.p.unit(), f, l).map(|r| u.text(r)))
            .collect();
        assert!(colon.iter().any(|t| t == ":"), "{colon:?}");
        assert!(colon.iter().any(|t| t == ": arr"), "{colon:?}");
    });
}

/// `Unit::token_end` is Kiln's `token_end` with the unit's lexer options
/// and interner: an identifier, a literal with a suffix, a token spliced
/// over a line, a macro token measured where it is spelled.
#[test]
fn s5_4_unit_token_end() {
    let src = "#define ID(x) x\nint bar = 1;\nauto s = \"ab\"_x;\nint ba\\\nz;\nint q = ID(bar);\n";
    unit(src, &[], |u| {
        let len = |i: usize| {
            let lo = u.toks[i].span.lo;
            u.p.unit().token_end(lo).and_then(|e| u.sm().same_entry_distance(lo, e))
        };
        assert_eq!(len(u.nth("bar", 0)), Some(3));
        assert_eq!(len(u.nth("\"ab\"_x", 0)), Some(6));
        assert_eq!(len(u.nth("ba\\\nz", 0)), Some(5), "the splice is inside the token");
        let arg = u.nth("bar", 1);
        assert!(u.toks[arg].span.lo.is_macro());
        assert_eq!(len(arg), Some(3));
        for t in u.toks {
            assert_eq!(u.p.unit().token_end(t.span.lo), Some(t.span.hi), "every token's own end");
        }
    });
}

/// `Unit::token_end` lexes with the unit's options, not Kiln's defaults:
/// in C++26 with reflection (the snippet's default there) `^^` is one
/// token, and two (`^` `^`) in Kiln's default lexer options.
#[test]
fn s5_4_unit_token_end_uses_the_unit_lexer_options() {
    let mut src = Source::new("constexpr auto r = ^^int;\n", &[]);
    src.std = "c++26".to_owned();
    with_unit(&src, |u| {
        let k = u.nth("^^", 0);
        assert_eq!(u.p.unit().token_end(u.toks[k].span.lo), Some(u.toks[k].span.hi));
        let r = file_range(u.p.unit(), u.toks[k].span.lo, u.toks[k].span.lo).expect("a range");
        assert_eq!((r.hi - r.lo, r.tier), (2, T0));
    })
    .unwrap_or_else(|e| panic!("{e}"));
}

// ---------------------------------------------------------------------
// Test 5: unit order.

/// Every token of `u`, sorted by `unit_order`, is the token sequence;
/// adjacent pairs are strictly ordered and the order is antisymmetric.
fn check_total_order(u: &Seen<'_, '_>) {
    let sm = u.sm();
    let mut locs: Vec<Loc> = u.toks.iter().map(|t| t.span.lo).collect();
    let in_order = locs.clone();
    // Some locations that are no token of the sequence: every byte of
    // every buffer, and every location of every expansion entry.
    for f in sm.file_ids() {
        let len = u32::try_from(sm.text(f).len()).unwrap();
        for o in 0..=len {
            locs.push(sm.loc(f, o));
        }
    }
    for e in sm.expansion_entries(0) {
        for o in 0..=e.info.len {
            locs.push(e.info.base.with_offset(o));
        }
    }
    locs.reverse();
    locs.sort_by(|&a, &b| unit_order(sm, a, b));
    locs.dedup();
    for w in locs.windows(2) {
        assert_eq!(unit_order(sm, w[0], w[1]), Ordering::Less, "{w:?}");
        assert_eq!(unit_order(sm, w[1], w[0]), Ordering::Greater, "{w:?}");
    }
    for &l in &locs {
        assert_eq!(unit_order(sm, l, l), Ordering::Equal);
    }
    let tokens: HashSet<Loc> = in_order.iter().copied().collect();
    let sorted: Vec<Loc> = locs.into_iter().filter(|l| tokens.contains(l)).collect();
    assert_eq!(sorted, in_order, "sorted, the tokens are the token sequence");
}

/// Test 5, `spec/api.md` §1: tokens in a header before and after a
/// nested include.
#[test]
fn s1_test5_unit_order_nested_include() {
    let a = "int a1;\n#include \"b.h\"\nint a2;\n";
    let b = "int b1;\n";
    unit("int m0;\n#include \"a.h\"\nint m1;\n", &[("a.h", a), ("b.h", b)], |u| {
        let at = |t: &str| u.toks[u.nth(t, 0)].span.lo;
        let order = ["m0", "a1", "b1", "a2", "m1"];
        for w in order.windows(2) {
            assert_eq!(unit_order(u.sm(), at(w[0]), at(w[1])), Ordering::Less, "{w:?}");
            assert_eq!(unit_order(u.sm(), at(w[1]), at(w[0])), Ordering::Greater, "{w:?}");
        }
        check_total_order(u);
    });
}

/// Test 5: tokens of one expansion; an argument used twice is at two
/// positions.
#[test]
fn s1_test5_unit_order_expansions() {
    let src = "#define E x y z\n#define T(v) v + v\n#define F(p) (p)\nint x, y, z, w;\n\
               int q = 0 E 1;\nint r = T(w) - F(T(w));\n";
    unit(src, &[], |u| {
        let at = |t: &str, n: usize| u.toks[u.nth(t, n)].span.lo;
        let lt = |a: Loc, b: Loc| unit_order(u.sm(), a, b) == Ordering::Less;
        assert!(lt(at("0", 0), at("x", 1)) && lt(at("x", 1), at("y", 1)));
        assert!(lt(at("y", 1), at("z", 1)) && lt(at("z", 1), at("1", 0)));
        let (w1, plus, w2) = (at("w", 1), at("+", 0), at("w", 2));
        assert!(lt(w1, plus) && lt(plus, w2), "the two uses of `w` are two positions");
        assert!(lt(w2, at("-", 0)));
        check_total_order(u);
    });
}

/// Test 5, locations that are no token: an invocation comes before the
/// tokens it expands to, and an `#include` before the included file's
/// tokens (a chain that is a prefix of the other comes first).
#[test]
fn s1_test5_unit_order_invocation_and_include_come_first() {
    let src = "#define E x y\nint x, y;\nint q = (E);\n#include \"h.h\"\n";
    unit(src, &[("h.h", "int h1;\n")], |u| {
        let sm = u.sm();
        let main = u.p.unit().main_file().expect("a main file");
        let off = |needle: &str| {
            sm.loc(main, u32::try_from(sm.text(main).find(needle).unwrap()).unwrap())
        };
        let at = |t: &str, n: usize| u.toks[u.nth(t, n)].span.lo;
        let (open, inv, close) = (at("(", 0), off("E);"), at(")", 0));
        let (x, y) = (at("x", 1), at("y", 1));
        assert!(x.is_macro() && y.is_macro());
        for (a, b) in [(open, inv), (inv, x), (x, y), (y, close)] {
            assert_eq!(unit_order(sm, a, b), Ordering::Less, "{a:?} {b:?}");
            assert_eq!(unit_order(sm, b, a), Ordering::Greater, "{a:?} {b:?}");
        }
        let (h1, include) = (at("h1", 0), sm.include_loc(at("h1", 0)));
        assert!(include.is_valid());
        assert_eq!(unit_order(sm, include, h1), Ordering::Less);
        assert_eq!(unit_order(sm, h1, include), Ordering::Greater);
    });
}

/// Test 5, the predefines in a snippet, whose main file is an in-memory
/// buffer like `<built-in>` (a snippet takes no `-include`; the worker's
/// forced include is `tests/parse.rs`
/// `s1_unit_order_forced_include_comes_before_the_main_file`): every
/// location of `<built-in>` comes before every location of the main file.
#[test]
fn s1_test5_unit_order_predefines_before_the_main_file() {
    unit("int m0;\n", &[], |u| {
        let sm = u.sm();
        let main = u.p.unit().main_file().expect("a main file");
        let builtin = sm.file_ids().find(|&f| sm.file_name(f) == "<built-in>").expect("<built-in>");
        assert!(builtin.index() > main.index());
        let bytes = |f| (0..=u32::try_from(sm.text(f).len()).unwrap()).map(move |o| sm.loc(f, o));
        for a in bytes(builtin) {
            for b in bytes(main) {
                assert_eq!(unit_order(sm, a, b), Ordering::Less, "{a:?} {b:?}");
            }
        }
    });
}

/// Test 5: the invalid location comes before every location, and is
/// equal to itself (`spec/landed/api/WP-084.md`).
#[test]
fn s1_test5_unit_order_invalid_first() {
    unit("#define E 1\nint m = E;\n#include \"h.h\"\n", &[("h.h", "int h;\n")], |u| {
        let sm = u.sm();
        assert_eq!(unit_order(sm, Loc::INVALID, Loc::INVALID), Ordering::Equal);
        let mut all: Vec<Loc> = u.toks.iter().map(|t| t.span.lo).collect();
        for f in sm.file_ids() {
            all.push(sm.loc(f, 0));
        }
        for l in all {
            assert_eq!(unit_order(sm, Loc::INVALID, l), Ordering::Less, "{l:?}");
            assert_eq!(unit_order(sm, l, Loc::INVALID), Ordering::Greater, "{l:?}");
        }
    });
}

/// Test 5 at depth: eight nested includes, and a token that is an
/// argument of six nested invocations at the bottom; chains longer than
/// a few links order as the token sequence.
#[test]
fn s1_test5_unit_order_deep_nesting() {
    let mut headers: Vec<(String, String)> = (1..8)
        .map(|k| (format!("h{k}.h"), format!("int a{k};\n#include \"h{}.h\"\nint b{k};\n", k + 1)))
        .collect();
    headers.push((
        "h8.h".to_owned(),
        "#define M1(x) (x)\n#define M2(x) M1(x)\n#define M3(x) M2(x)\n#define M4(x) M3(x)\n\
         #define M5(x) M4(x)\n#define M6(x) M5(x)\nint deep = M6(M6(7) + 1);\n"
            .to_owned(),
    ));
    let h: Vec<(&str, &str)> = headers.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    unit("int m0;\n#include \"h1.h\"\nint m1;\n", &h, |u| {
        let at = |t: &str| u.toks[u.nth(t, 0)].span.lo;
        let lt = |a, b| unit_order(u.sm(), a, b) == Ordering::Less;
        assert!(at("7").is_macro());
        assert!(lt(at("a7"), at("7")) && lt(at("7"), at("1")) && lt(at("1"), at("b7")));
        assert!(lt(at("7"), at("m1")) && lt(at("m0"), at("7")));
        check_total_order(u);
    });
}

/// Test 5: the order is total over a source with includes, expansions,
/// arguments split by expansions, `#` and `##`.
#[test]
fn s1_test5_unit_order_total() {
    let h = "#define H(a, b) a ## b + #a\nint hh = H(x, y)[0];\n";
    let src = "#define ONE 1\n#define F(x) (x)\n#define G(x) F(x) F(x)\nint xy, k, m;\n\
               #include \"h.h\"\nint v = F(k ONE m) + G(ONE);\n#include \"h.h\"\nint w;\n";
    unit(src, &[("h.h", h)], check_total_order);
}

// ---------------------------------------------------------------------
// Test 4: the property.

/// What [`property`] counted.
#[derive(Default, Debug)]
struct Tally {
    /// Ranges checked, by tier.
    ranges: [u64; 2],
    /// Spans (distinct first and last token) given no range, by reason.
    none: BTreeMap<&'static str, u64>,
    /// Expressions seen.
    exprs: u64,
    /// Errors the front end reported for the units.
    errors: u64,
    /// Units the harness could not use, by reason.
    skipped: BTreeMap<String, u64>,
    /// References and declaration names of test 6.
    refs: u64,
    /// The declaration names among them.
    names: u64,
    /// Those whose end is the end of no token of the sequence.
    refs_unchecked: u64,
    /// Those `file_range` refuses because their last token may be split.
    refs_split: u64,
    failures: Vec<String>,
    /// With `SLIP_RANGES_DETAIL` set: each span with no range or not
    /// checked, and why.
    notes: Vec<String>,
}

impl Tally {
    fn add(&mut self, o: Tally) {
        for t in 0..2 {
            self.ranges[t] += o.ranges[t];
        }
        for (k, v) in o.none {
            *self.none.entry(k).or_default() += v;
        }
        for (k, v) in o.skipped {
            *self.skipped.entry(k).or_default() += v;
        }
        self.exprs += o.exprs;
        self.errors += o.errors;
        self.refs += o.refs;
        self.names += o.names;
        self.refs_unchecked += o.refs_unchecked;
        self.refs_split += o.refs_split;
        self.failures.extend(o.failures);
        self.notes.extend(o.notes);
    }

    fn report(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "expressions {} (front-end errors {}); ranges checked: tier 0 {}, tier 1 {}; failures {}",
            self.exprs,
            self.errors,
            self.ranges[0],
            self.ranges[1],
            self.failures.len()
        );
        let _ = writeln!(s, "spans with no range, by reason: {:?}", self.none);
        let _ = writeln!(s, "units skipped, by reason: {:?}", self.skipped);
        let _ = writeln!(
            s,
            "test 6: references and names {} (names {}), unchecked {}, refused as a possibly split `>` {}",
            self.refs, self.names, self.refs_unchecked, self.refs_split
        );
        s
    }
}

/// The file location that produced the token at `loc`: through argument
/// entries to where the argument is written, through macro entries to
/// the invocation (so a token of a macro invoked inside an argument is
/// produced by the bytes of that invocation). An oracle of this test,
/// not a rule of `ranges`.
fn producer(sm: &SourceManager, loc: Loc) -> Option<(FileId, u32)> {
    sm.offset_of(producer_loc(sm, loc)?)
}

/// [`producer`] as a location (in the inclusion that holds it).
fn producer_loc(sm: &SourceManager, mut loc: Loc) -> Option<Loc> {
    for _ in 0..10_000 {
        if !loc.is_macro() {
            return loc.is_valid().then_some(loc);
        }
        let e = sm.expansion_info(loc)?;
        loc = match e.kind {
            ExpansionKind::MacroArg => sm.immediate_spelling_loc(loc),
            ExpansionKind::Macro { .. } => e.expansion.lo,
        };
    }
    None
}

/// Which use of an argument the token at `loc` belongs to: the
/// parameter its innermost argument entry replaced (walking out from the
/// token's own entry, through macro entries to their invocation). Two
/// tokens of one use have the same key; two uses of an argument replace
/// two parameter tokens.
fn use_key(sm: &SourceManager, mut loc: Loc) -> Option<Loc> {
    for _ in 0..10_000 {
        let e = sm.expansion_info(loc)?;
        match e.kind {
            ExpansionKind::MacroArg => return Some(e.expansion.lo),
            ExpansionKind::Macro { .. } => loc = e.expansion.lo,
        }
    }
    None
}

/// The outermost invocation `loc` is in, as file offsets.
fn invocation(sm: &SourceManager, loc: Loc) -> Option<(FileId, u32, u32)> {
    let s = sm.expansion_span(loc);
    let (f, lo) = sm.offset_of(s.lo)?;
    let (g, hi) = sm.offset_of(s.hi)?;
    (f == g).then_some((f, lo, hi))
}

/// The marker that replaces the bytes of a range: an identifier no
/// snippet has, between blanks (so it joins no neighbouring token), then
/// the line breaks of the bytes it replaces (so no line number after it
/// moves).
fn marker(replaced: &str) -> (String, String) {
    let name = "__slip_wp084_marker__".to_owned();
    let mut with = format!(" {name} ");
    with.extend(replaced.chars().filter(|&c| c == '\n'));
    (name, with)
}

/// The token spellings of `src` with the bytes of `r` replaced by the
/// marker ([`marker`]): what [`check_range`] compares with.
fn marked(src: &Source, u: &Seen<'_, '_>, r: FileRange) -> Result<Seq, String> {
    marked_by(src, &marking(u, r)?)
}

/// The edit [`marked`] makes: the snippet path, the bytes, the marker.
fn marking(u: &Seen<'_, '_>, r: FileRange) -> Result<(String, usize, usize, String), String> {
    let path = u.paths.get(&r.file).ok_or("the range is in no snippet file")?;
    let (_, with) = marker(&u.sm().text(r.file)[r.lo as usize..r.hi as usize]);
    Ok((path.clone(), r.lo as usize, r.hi as usize, with))
}

fn marked_by(src: &Source, m: &(String, usize, usize, String)) -> Result<Seq, String> {
    src.replaced(&m.0, m.1, m.2, &m.3).spellings()
}

/// The number of threads of a test that runs many units:
/// `SLIP_TEST_THREADS`, else at most 4.
fn test_threads() -> usize {
    match std::env::var("SLIP_TEST_THREADS") {
        Ok(v) => v.parse::<usize>().expect("SLIP_TEST_THREADS: a positive number").max(1),
        Err(_) => std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4),
    }
}

/// `f` of each item, on `threads` threads, in order.
fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut out: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.min(items.len()).max(1))
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(x) = items.get(k) else { break };
                        mine.push((k, f(x)));
                    }
                    mine
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap()).collect()
    });
    out.sort_by_key(|(k, _)| *k);
    out.into_iter().map(|(_, r)| r).collect()
}

/// Why a span was given no range.
fn none_reason(u: &Seen<'_, '_>, first: Loc, last: Loc) -> &'static str {
    if !first.is_valid() || !last.is_valid() {
        return "invalid location";
    }
    let unit = u.p.unit();
    if !unit.is_token_start(first) || !unit.is_token_start(last) {
        return "an end inside a token";
    }
    let Some(end) = unit.token_end(last) else { return "no token ends the span" };
    if may_be_split(u.sm(), last, end) {
        return "interim: the last token may be a split `>`";
    }
    // Kiln gave a range that `file_range_of_span` refused.
    if let Some((f, lo, hi)) = u.sm().editable_file_range(Span::new(first, end)) {
        if lo >= hi {
            return "an empty or reversed span";
        }
        if u.sm().is_written_in_scratch_space(u.sm().loc(f, lo)) {
            return "a scratch buffer";
        }
        return "interim: the range holds or is in a directive line";
    }
    match (first.is_macro(), last.is_macro()) {
        (false, false) => "kiln: file to file (two buffers, or out of order)",
        (true, false) => "kiln: from inside an expansion",
        (false, true) => "kiln: into an expansion",
        (true, true) => "kiln: within or across expansions",
    }
}

/// Checks the property of test 4 for one range `r` of tokens `i..=j` of
/// `u`; `Err` says how it fails. The marked source reports the same
/// preprocessing errors; the tokens of every other file are unchanged;
/// and the property holds in the entry of the file that holds the span
/// ([`Entry`]). Other entries of the same file are not checked: what an
/// edit of a file entered twice does is for the votes (`spec/edits.md`
/// §4.3).
fn check_range(
    u: &Seen<'_, '_>,
    before: &Seq,
    after: &Result<Seq, String>,
    r: FileRange,
    i: usize,
    j: usize,
) -> Result<(), String> {
    let after = after.as_ref().map_err(Clone::clone)?;
    if after.errors != before.errors {
        return Err(format!(
            "the marked source has {} preprocessing errors, the source {}",
            after.errors, before.errors
        ));
    }
    let file = u.paths.get(&r.file).map_or("<?>", String::as_str);
    let outside = |q: &Seq| -> Vec<String> {
        q.text.iter().zip(&q.entry).filter(|(_, e)| e.0 != file).map(|(t, _)| t.clone()).collect()
    };
    let (b, a) = (outside(before), outside(after));
    if a != b {
        return Err(diff("the tokens of other files", &b, &a));
    }
    let entry = &before.entry[i];
    if before.entry[j] != *entry {
        return Err("the span is in two entries".into());
    }
    let sel: Vec<usize> = (0..before.text.len()).filter(|&k| before.entry[k] == *entry).collect();
    let toks: Vec<Token> = sel.iter().map(|&k| u.toks[k]).collect();
    let b: Vec<String> = sel.iter().map(|&k| before.text[k].clone()).collect();
    let a: Vec<String> = after
        .text
        .iter()
        .zip(&after.entry)
        .filter(|(_, e)| *e == entry)
        .map(|(t, _)| t.clone())
        .collect();
    let at = |k: usize| sel.binary_search(&k).expect("a token of the entry");
    check_entry(u, &toks, &b, &a, r, at(i), at(j))
}

/// [`check_range`] within one entry: `toks` its tokens, `before` and
/// `after` their spellings before and after the marking, `i..=j` the span.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "one oracle, tier by tier; `i..=j` the span, `(f, a, b)` its invocation"
)]
fn check_entry(
    u: &Seen<'_, '_>,
    toks: &[Token],
    before: &[String],
    after: &[String],
    r: FileRange,
    i: usize,
    j: usize,
) -> Result<(), String> {
    let sm = u.sm();
    let (name, _) = marker(&sm.text(r.file)[r.lo as usize..r.hi as usize]);
    let in_r = |t: &Token| {
        producer(sm, t.span.lo).is_some_and(|(f, o)| f == r.file && r.lo <= o && o < r.hi)
    };
    match r.tier {
        Tier::File => {
            // Exactly the span became the marker.
            let mut want: Vec<String> = before[..i].to_vec();
            want.push(name.clone());
            want.extend_from_slice(&before[j + 1..]);
            if after != want.as_slice() {
                return Err(diff("tier 0", &want, after));
            }
            // Outside every argument: no invocation holds part of it.
            for t in toks {
                if !t.span.lo.is_macro() {
                    continue;
                }
                if let Some((f, a, b)) = invocation(sm, t.span.lo)
                    && f == r.file
                    && a < r.hi
                    && r.lo < b
                    && !(r.lo <= a && b <= r.hi)
                {
                    return Err(format!(
                        "tier 0 range {}..{} is inside the invocation {a}..{b}",
                        r.lo, r.hi
                    ));
                }
            }
        }
        Tier::MacroArgument => {
            let s = &toks[i..=j];
            if !s.iter().all(|t| t.span.lo.is_macro() && in_r(t)) {
                return Err("tier 1: a token of the span is not produced by the range".into());
            }
            let Some((f, a, b)) = invocation(sm, s[0].span.lo) else {
                return Err("tier 1: the span is in no invocation".into());
            };
            if f != r.file || !(a <= r.lo && r.hi <= b && (a, b) != (r.lo, r.hi)) {
                return Err(format!(
                    "tier 1 range {}..{} is not inside its invocation {a}..{b}",
                    r.lo, r.hi
                ));
            }
            // The expansion of that invocation is a run of the sequence;
            // nothing outside it changes.
            let of_v =
                |t: &Token| t.span.lo.is_macro() && invocation(sm, t.span.lo) == Some((f, a, b));
            let (mut x0, mut x1) = (i, j);
            while x0 > 0 && of_v(&toks[x0 - 1]) {
                x0 -= 1;
            }
            while x1 + 1 < toks.len() && of_v(&toks[x1 + 1]) {
                x1 += 1;
            }
            let tail = before.len() - x1 - 1;
            if after.len() < x0 + tail
                || after[..x0] != before[..x0]
                || after[after.len() - tail..] != before[x1 + 1..]
            {
                return Err(diff("tier 1, outside the invocation", before, after));
            }
            // Inside it: each use of the argument became the marker; a
            // `#` or `##` result may change; nothing else does.
            let mut want: Vec<Option<String>> = Vec::new();
            let mut prev: Option<Option<Loc>> = None;
            let mut span_is_a_use = false;
            let key = |t: &Token| use_key(sm, t.span.lo);
            for (k, t) in toks.iter().enumerate().take(x1 + 1).skip(x0) {
                if in_r(t) {
                    let new_use = prev != Some(key(t));
                    if new_use {
                        want.push(Some(name.clone()));
                        span_is_a_use |= k == i;
                    } else if k == i {
                        return Err("tier 1: the span begins inside a use of the argument".into());
                    }
                    prev = Some(key(t));
                } else {
                    prev = None;
                    if sm.is_written_in_scratch_space(sm.spelling_loc(t.span.lo)) {
                        want.push(None);
                    } else {
                        want.push(Some(before[k].clone()));
                    }
                }
            }
            if toks.get(j + 1).is_some_and(|n| in_r(n) && key(n) == key(&toks[j])) {
                return Err("tier 1: the span ends inside a use of the argument".into());
            }
            if !span_is_a_use {
                return Err("tier 1: the span is not one use of the argument".into());
            }
            let got = &after[x0..after.len() - tail];
            let fits = got.len() == want.len()
                && got.iter().zip(&want).all(|(g, w)| w.as_ref().is_none_or(|w| w == g));
            if !fits {
                let want: Vec<String> =
                    want.into_iter().map(|w| w.unwrap_or_else(|| "<# or ##>".into())).collect();
                return Err(diff("tier 1, inside the invocation", &want, got));
            }
        }
    }
    Ok(())
}

/// The first difference of two sequences, with context.
fn diff(what: &str, want: &[String], got: &[String]) -> String {
    let k = want.iter().zip(got).take_while(|(a, b)| a == b).count();
    let ctx = |v: &[String]| v[k.saturating_sub(3)..(k + 6).min(v.len())].join(" ");
    format!(
        "{what}: sequences differ at token {k} (want {} tokens, got {}):\n  want … {}\n  got  … {}",
        want.len(),
        got.len(),
        ctx(want),
        ctx(got)
    )
}

/// Test 4 over one unit: every expression's span (its first and last
/// token, `Ast::begin_loc`, `Ast::end_loc`), each distinct span once; and
/// test 6 over every written reference and declaration name.
fn property(src: &Source, label: &str) -> Tally {
    property_of(src, label, None, 1)
}

/// [`property`] over the expressions of the kinds `only` accepts when
/// given, else every expression.
#[allow(clippy::too_many_lines, reason = "one pass over a unit: tests 4 and 6")]
fn property_of(
    src: &Source,
    label: &str,
    only: Option<fn(ExprKind) -> bool>,
    threads: usize,
) -> Tally {
    let mut tally = Tally::default();
    let res = with_unit(src, |u| {
        let mut t = Tally { errors: u64::from(u.p.errors), ..Tally::default() };
        let ast = u.p.ast;
        let sm = u.sm();
        let index: HashMap<Loc, usize> =
            u.toks.iter().enumerate().map(|(k, t)| (t.span.lo, k)).collect();
        let mut seen: HashSet<(Loc, Loc)> = HashSet::new();
        let before = Seq::of(sm, u.toks, |f| u.paths.get(&f).cloned(), u.pp_errors);
        let progress = only.is_some();
        let mut pending: Vec<(ExprId, FileRange, usize, usize)> = Vec::new();
        let exprs = (1..=ast.exprs.len())
            .filter_map(|k| ExprId::try_new(u32::try_from(k).ok()?))
            .filter(|&e| only.is_none_or(|f| f(ast.exprs.get(e).kind())));
        for e in exprs {
            t.exprs += 1;
            let (first, last) = (ast.begin_loc(e), ast.end_loc(e));
            if !seen.insert((first, last)) {
                continue;
            }
            let detail = std::env::var_os("SLIP_RANGES_DETAIL").is_some();
            let kind = ast.exprs.get(e).kind();
            let Some(r) = file_range(u.p.unit(), first, last) else {
                let why = none_reason(u, first, last);
                *t.none.entry(why).or_default() += 1;
                if detail && why != "invalid location" && !why.starts_with("kiln: within") {
                    t.notes.push(format!(
                        "{label}: no range, {why}: {kind:?} {:?}..{:?}",
                        sm.span_text(Span::new(first, first.with_offset(8))),
                        sm.span_text(Span::new(last, last.with_offset(8)))
                    ));
                }
                continue;
            };
            // A range is given only for tokens: one the property cannot
            // check is a failure.
            let (Some(&i), Some(&j)) = (index.get(&first), index.get(&last)) else {
                t.failures.push(format!(
                    "{label}: a range for a span whose ends are no tokens of the sequence: {kind:?} `{}`",
                    u.text(r)
                ));
                continue;
            };
            if i > j {
                t.failures.push(format!(
                    "{label}: a range for tokens out of order ({i} > {j}): {}",
                    u.text(r)
                ));
                continue;
            }
            t.ranges[r.tier as usize] += 1;
            pending.push((e, r, i, j));
        }
        // Re-preprocess the marked sources `threads` at a time, a batch at
        // a time (one sequence of a whole unit is large), and check each.
        let size = threads * 16;
        for (b, batch) in pending.chunks(size).enumerate() {
            let edits: Vec<Result<_, String>> =
                batch.iter().map(|&(_, r, _, _)| marking(u, r)).collect();
            let afters = if threads == 1 {
                edits.iter().map(|m| marked_by(src, m.as_ref().map_err(Clone::clone)?)).collect()
            } else {
                par_map(&edits, threads, |m| marked_by(src, m.as_ref().map_err(Clone::clone)?))
            };
            for (&(e, r, i, j), after) in batch.iter().zip(&afters) {
                if let Err(why) = check_range(u, &before, after, r, i, j) {
                    let toks: Vec<String> = u.toks[i..=j].iter().map(|t| spelling(sm, t)).collect();
                    t.failures.push(format!(
                        "{label}: expression {} `{}`, range `{}` tier {:?}: {why}",
                        e.index(),
                        toks.join(" "),
                        u.text(r),
                        r.tier
                    ));
                }
            }
            if progress {
                let done = b * size + batch.len();
                eprintln!(
                    "{label}: {done} of {} ranges checked, {} failures",
                    pending.len(),
                    t.failures.len()
                );
            }
        }
        // Test 6: file_range against file_range_of_span for every
        // reference and every declaration name.
        let ends: HashMap<Loc, Loc> = u.toks.iter().map(|t| (t.span.hi, t.span.lo)).collect();
        let spans = ast
            .written_refs
            .iter()
            .map(|(_, w)| (w.range, false))
            .chain(ast.decl_extents.iter().map(|(_, d)| (d.name, true)))
            .filter(|(s, _)| s.lo.is_valid());
        for (s, name) in spans {
            t.refs += 1;
            t.names += u64::from(name);
            let Some(&last) = ends.get(&s.hi) else {
                t.refs_unchecked += 1;
                continue;
            };
            let a = file_range(u.p.unit(), s.lo, last);
            let split = may_be_split(sm, last, s.hi);
            let b = if split { None } else { file_range_of_span(sm, s) };
            t.refs_split += u64::from(split);
            if a != b {
                t.failures.push(format!("{label}: test 6: {a:?} != {b:?} for {s:?}"));
            }
        }
        t
    });
    match res {
        Ok(t) => tally.add(t),
        Err(e) => {
            let reason = if e.contains("differ") || e.contains("expansion entries") {
                "the parse and the preprocessing disagree".to_owned()
            } else {
                e.lines().next().unwrap_or_default().to_owned()
            };
            *tally.skipped.entry(reason).or_default() += 1;
        }
    }
    tally
}

/// Macro-heavy snippets for test 4, besides the pair tests.
const PROPERTY_SNIPPETS: &[&str] = &[
    "#define ID(x) x\n#define F(x) (x)\n#define T(x) x + x\n#define U(x) (x + x)\n\
     #define G(p, q) p * q\n#define ONE 1\n#define CALL(f, a) f(a)\n\
     int foo(int); int a, b, c;\n\
     int v1 = ID(foo)(a) + F(a + b) + T(c) + U(a) + G(a, b + 1) + ONE;\n\
     int v2 = CALL(foo, a + ONE) + F(F(b) * ONE) + foo(T(a));\n\
     int v3 = F(ID(a) + ID(foo(b))) - G(F(a), T(b));\n",
    "#define S(x) #x\n#define S2(x) x, #x\n#define CAT(a, b) a ## b\n\
     #define PICK(a, ...) (a __VA_OPT__(+) __VA_ARGS__)\n\
     int xy, z; const char* s0 = S(abc);\n\
     int f(int, const char*); int v = f(S2(xy)) + CAT(x, y) + PICK(1) + PICK(2, z, 3);\n",
    "#define MAX(a, b) ((a) > (b) ? (a) : (b))\n#define APPLY(m, x) m(x)\n#define NEG(x) (-(x))\n\
     struct P { int m; int g() const { return m; } };\n\
     int h(P p, int k) { return MAX(p.g(), k) + APPLY(NEG, p.m) + MAX(MAX(1, 2), \
     NEG(k)); }\n",
    "#define BODY return a\n#define ARG2(x, y) y\n#define LP (\n#define FN h\n\
     int h(int);\nint q(int a) { int r = FN(a) + ARG2(a, a + 1) + h LP 3); BODY; }\n",
    // Raw order is not text order: from a macro token into file text.
    "#define P 1 +\n#define ID(x) x\nint foo(int); int a;\nint v = P 2;\nint w = ID(foo)(a) + P ID(a);\n",
    // Ends inside a token (decision 6), a possibly split `>`, directives.
    "unsigned long long operator\"\"_k(unsigned long long);\nauto a = 12_k + 1 + (2_k);\n\
     template <int N> constexpr int vt = N;\ntemplate <int N> struct B { static constexpr int c = N; };\n\
     B<vt<1>> y;\nint z = B<vt<B<1>::c>>::c + (4 >> 1);\n\
     int f(int, int); int b, c;\nint v = f(1,\n#ifdef X\n b\n#else\n c\n#endif\n) + (1\n#if 1\n+ 2\n#endif\n);\n",
];

/// Test 4 on the snippets above, with headers and an expansion split by
/// an inner expansion.
#[test]
fn s5_4_test4_property_on_macro_snippets() {
    let mut tally = Tally::default();
    let h = "#define HF(x) (x)\ninline int hv(int a) { return HF(a + 1) * 2; }\n";
    let main = "#include \"h.h\"\n#define K 3\nint w = hv(K) + HF(hv(2) + K);\n";
    let sources = PROPERTY_SNIPPETS
        .iter()
        .enumerate()
        .map(|(k, s)| (format!("snippet {k}"), Source::new(s, &[])))
        .chain([("with a header".to_owned(), Source::new(main, &[("h.h", h)]))]);
    let mut counts = Vec::new();
    for (label, src) in sources {
        let t = property(&src, &label);
        counts.push([t.ranges[0], t.ranges[1], t.none.values().sum(), t.names]);
        tally.add(t);
    }
    eprintln!("per source [tier 0, tier 1, no range, names]: {counts:?}");
    // A lost range is a failure: each source's counts are pinned (and the
    // declaration names of test 6 are seen, so the tables were asked for).
    let want: [[u64; 4]; 7] = [
        [20, 29, 6, 7],
        [7, 5, 5, 5],
        [6, 24, 30, 6],
        [8, 2, 2, 4],
        [7, 0, 3, 4],
        [21, 0, 12, 15],
        [8, 8, 0, 3],
    ];
    assert_eq!(counts, want, "[tier 0, tier 1, no range, names] per source\n{}", tally.report());
    eprint!("{}", tally.report());
    assert!(tally.failures.is_empty(), "{}\n{}", tally.failures.join("\n"), tally.report());
    assert!(tally.skipped.is_empty(), "{}", tally.report());
    assert!(tally.ranges[0] > 40 && tally.ranges[1] > 40, "{}", tally.report());
    assert!(
        tally.refs > 40 && tally.refs_unchecked == 0,
        "test 6 saw the tables: {}",
        tally.report()
    );
}

/// Decision 2: a span Kiln maps to a reversed run (an expansion's first
/// token to file text before the invocation) has no range.
#[test]
fn s5_4_test3_none_a_span_kiln_maps_backwards() {
    unit("#define P 1 +\nint v = 2 + P 3;\n", &[], |u| {
        let (one, two) = (u.toks[u.nth("1", 0)].span, u.toks[u.nth("2", 0)].span);
        assert_eq!(file_range_of_span(u.sm(), Span::new(one.lo, two.hi)), None);
        // Kiln itself gives none here at the pin: the check after Kiln
        // is a guard (`spec/landed/api/WP-084.md` decision 2).
        assert_eq!(u.sm().editable_file_range(Span::new(one.lo, two.hi)), None);
    });
}

/// The checker of test 4 compares the preprocessing errors: a range
/// holding `#if` without its `#endif`, which the marked source turns into
/// an unmatched `#endif` and the same tokens.
#[test]
fn s5_4_test4_checker_catches_new_preprocessing_errors() {
    let src = Source::new("int v = 1\n#if 1\n+ 2\n#endif\n;\n", &[]);
    with_unit(&src, |u| {
        let before = Seq::of(u.sm(), u.toks, |f| u.paths.get(&f).cloned(), u.pp_errors);
        let (i, j) = (u.nth("1", 0), u.nth("2", 0));
        let main = u.p.unit().main_file().unwrap();
        let text = u.sm().text(main);
        let lo = u32::try_from(text.find("1\n").unwrap()).unwrap();
        let hi = u32::try_from(text.find("2\n").unwrap() + 1).unwrap();
        let r = FileRange { file: main, lo, hi, tier: Tier::File };
        let after = marked(&src, u, r);
        assert_eq!(
            after.as_ref().map(|a| a.text.clone()).ok(),
            Some(vec![
                "int".to_owned(),
                "v".into(),
                "=".into(),
                "__slip_wp084_marker__".into(),
                ";".into()
            ]),
            "the same tokens"
        );
        let e = check_range(u, &before, &after, r, i, j).expect_err("a new error");
        assert!(e.contains("preprocessing errors"), "{e}");
    })
    .unwrap();
}

/// The checker of test 4 catches a wrong range: replacing one byte too
/// many, or the wrong tier.
#[test]
fn s5_4_test4_checker_catches_wrong_ranges() {
    let src = Source::new("#define F(x) (x)\nint a, b;\nint v = F(a + b) + a;\n", &[]);
    with_unit(&src, |u| {
        let before = Seq::of(u.sm(), u.toks, |f| u.paths.get(&f).cloned(), u.pp_errors);
        let (i, j) = (u.nth("a", 1), u.nth("b", 1));
        let r = u.range(i, j).unwrap();
        assert_eq!(check_range(u, &before, &marked(&src, u, r), r, i, j), Ok(()));
        let long = FileRange { hi: r.hi + 1, ..r };
        assert!(
            check_range(u, &before, &marked(&src, u, long), long, i, j).is_err(),
            "a byte too many"
        );
        let short = FileRange { hi: r.hi - 1, ..r };
        assert!(
            check_range(u, &before, &marked(&src, u, short), short, i, j).is_err(),
            "a byte short"
        );
        let tier0 = FileRange { tier: Tier::File, ..r };
        assert!(
            check_range(u, &before, &marked(&src, u, tier0), tier0, i, j).is_err(),
            "tier 0 inside an argument"
        );
        let k = u.nth("a", 2);
        let plain = u.range(k, k).unwrap();
        assert_eq!(check_range(u, &before, &marked(&src, u, plain), plain, k, k), Ok(()));
        let tier1 = FileRange { tier: Tier::MacroArgument, ..plain };
        assert!(
            check_range(u, &before, &marked(&src, u, tier1), tier1, k, k).is_err(),
            "tier 1 outside a macro"
        );
        assert!(
            check_range(u, &before, &marked(&src, u, plain), plain, k - 1, k).is_err(),
            "more tokens"
        );
    })
    .unwrap();
}

/// Test 4 with a header entered twice, without a guard: the property
/// holds in each entry (`check_range`); what an edit of the file does
/// to the other entry is for the votes (`spec/edits.md` §4.3). The range
/// of a span in either entry is the header's bytes.
#[test]
fn s5_4_test4_property_a_header_entered_twice() {
    let h = "1 + 2\n";
    let main = "int x = \n#include \"h.h\"\n;\nint y = 3 *\n#include \"h.h\"\n;\n";
    let src = Source::new(main, &[("h.h", h)]);
    let t = property(&src, "a header entered twice");
    assert!(t.failures.is_empty(), "{}\n{}", t.failures.join("\n"), t.report());
    assert!(t.ranges[0] >= 4, "{}", t.report());
    with_unit(&src, |u| {
        let first = u.range(u.nth("1", 0), u.nth("2", 0)).expect("a range");
        let second = u.range(u.nth("1", 1), u.nth("2", 1)).expect("a range");
        assert_eq!(first, second);
        assert_eq!((u.text(first), first.tier), ("h.h:1 + 2".to_owned(), T0));
    })
    .unwrap();
}

/// Test 4 with an X-macro file entered under two definitions of `X`: an
/// argument written in the file is a tier 1 range in each entry.
#[test]
fn s5_4_test4_property_an_x_macro_file() {
    let def = "X(a, 1)\nX(b, 2)\n";
    let main = "#define X(n, v) int n = v;\n#include \"x.def\"\n#undef X\n\
                #define X(n, v) + v\nint s = 0\n#include \"x.def\"\n;\n";
    let src = Source::new(main, &[("x.def", def)]);
    let t = property(&src, "an X-macro file");
    assert!(t.failures.is_empty(), "{}\n{}", t.failures.join("\n"), t.report());
    assert!(t.ranges[1] >= 2, "{}", t.report());
    with_unit(&src, |u| {
        let one = u.range(u.nth("1", 0), u.nth("1", 0)).expect("a range");
        let again = u.range(u.nth("1", 1), u.nth("1", 1)).expect("a range");
        assert_eq!(one, again);
        assert_eq!((u.text(one), one.tier), ("x.def:1".to_owned(), T1));
    })
    .unwrap();
}

// ---------------------------------------------------------------------
// Test 4 over the digest's pair tests.

/// The digest's pair-test roots (`crates/slip_digest/tests/`).
const PAIR_ROOTS: &[&str] = &["differ", "equal", "demand", "indeterminate"];

/// Every directory under `root` holding a `t.cpp`, an `a.cpp` or a
/// `b.cpp`, sorted.
fn pair_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut mains = false;
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(e.file_name().to_str(), Some("t.cpp" | "a.cpp" | "b.cpp")) {
                mains = true;
            }
        }
        if mains {
            out.push(d);
        }
    }
    out.sort();
    out
}

/// The sources of the two sides of a pair test, as the digest's runner
/// builds them (`crates/slip_digest/tests/common/mod.rs`, `side_files`
/// and `header`): `side.h` for a `t.cpp`, every other file of the
/// directory, the files of `tests/support/` the directory lacks, and the
/// header lines of the main file.
fn pair_sides(dir: &Path, support: &[(String, String)]) -> Vec<Source> {
    let mut all: BTreeMap<String, String> = BTreeMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_file()
            && let (Some(n), Ok(t)) =
                (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
        {
            all.insert(n, t);
        }
    }
    let mut sides = Vec::new();
    for side in ["a", "b"] {
        let mut files = Vec::new();
        let main = if let Some(t) = all.get("t.cpp") {
            files
                .push(("side.h".to_owned(), if side == "a" { "" } else { "#define B 1\n" }.into()));
            t.clone()
        } else if let Some(m) = all.get(&format!("{side}.cpp")) {
            m.clone()
        } else {
            continue;
        };
        files.push(("main.cpp".to_owned(), main.clone()));
        for (n, t) in &all {
            if !matches!(n.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
                files.push((n.clone(), t.clone()));
            }
        }
        for (n, t) in support {
            if !all.contains_key(n) {
                files.push((n.clone(), t.clone()));
            }
        }
        let mut s = Source {
            files,
            main: "main.cpp".to_owned(),
            include: vec![String::new()],
            std: "c++20".to_owned(),
            args: Vec::new(),
            target: None,
        };
        for line in main.lines() {
            let line = line.trim_end();
            if line == "#include \"side.h\"" {
                continue;
            }
            let Some((key, value)) = line.strip_prefix("// ").and_then(|r| r.split_once(": "))
            else {
                break;
            };
            match key {
                "std" => value.clone_into(&mut s.std),
                "args" => {
                    s.args =
                        value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect();
                }
                "target" => s.target = Some(value.to_owned()),
                "errors" => {}
                _ => break,
            }
        }
        sides.push(s);
    }
    sides
}

/// Test 4 (and 6) over every expression of every snippet of the digest's
/// pair tests, both sides. `SLIP_TEST_THREADS` threads (at most 4 by
/// default), as the pair runners.
#[test]
fn s5_4_test4_property_over_digest_pair_tests() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../slip_digest/tests");
    let mut support: Vec<(String, String)> = Vec::new();
    for e in std::fs::read_dir(tests.join("support")).into_iter().flatten().flatten() {
        if let (Some(n), Ok(t)) =
            (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
        {
            support.push((n, t));
        }
    }
    support.sort();
    let dirs: Vec<PathBuf> = PAIR_ROOTS.iter().flat_map(|r| pair_dirs(&tests.join(r))).collect();
    assert!(dirs.len() > 1000, "found only {} pair-test directories", dirs.len());
    let threads = test_threads();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let tallies: Vec<Tally> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut t = Tally::default();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(dir) = dirs.get(k) else { break };
                        let label = dir.strip_prefix(&tests).unwrap_or(dir).display().to_string();
                        for (n, side) in pair_sides(dir, &support).iter().enumerate() {
                            t.add(property(side, &format!("{label} side {n}")));
                        }
                    }
                    t
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let mut tally = Tally::default();
    for t in tallies {
        tally.add(t);
    }
    tally.failures.sort();
    eprintln!("{} pair-test directories", dirs.len());
    for n in &tally.notes {
        eprintln!("{n}");
    }
    eprint!("{}", tally.report());
    assert!(tally.failures.is_empty(), "{}\n{}", tally.failures.join("\n"), tally.report());
}

// ---------------------------------------------------------------------
// Test 4 over LLVM units, by hand (`docs/wp/WP-084.md` "Done when").

/// A path as a snippet path: relative to the root, `.` and `..` resolved
/// by their spelling.
fn snippet_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    parts.join("/")
}

/// One LLVM unit of the corpus as a snippet: every buffer the real front
/// end read, under its path; the search list of the real front end's
/// header lookups; the predefines buffer as a header the main file
/// includes first; the language arguments the snippet takes. Only so can
/// the unit be preprocessed again with one file changed (Kiln reads
/// files from disk, and Slip has no overlay yet). `None` and why when
/// the unit is not in the database.
fn llvm_source(corpus: &Path, unit: &str) -> Result<Source, String> {
    use kiln_driver::front_end::{FrontEndOptions, run_front_end};
    let db =
        std::fs::read(corpus.join("build/compile_commands.json")).map_err(|e| e.to_string())?;
    let entries = slip_cdb::database::read_json(&db).map_err(|e| e.detail)?;
    let entry = entries
        .into_iter()
        .flatten()
        .find(|e| e.file.ends_with(unit))
        .ok_or_else(|| format!("{unit}: not in the database"))?;
    std::env::set_current_dir(&entry.directory).map_err(|e| e.to_string())?;
    let mut line = vec!["-fsyntax-only".to_owned()];
    let mut it = entry.arguments[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-c" => {}
            "-o" => {
                it.next();
            }
            _ => line.push(a.clone()),
        }
    }
    let mut options = FrontEndOptions::default();
    options.record_lookups = true;
    let report = run_front_end("kiln", &line, options, |fe| {
        let sm = fe.sm;
        let mut files: Vec<(String, String)> = Vec::new();
        let mut seen = HashSet::new();
        for f in sm.file_ids() {
            let name = sm.file_name(f);
            if name.starts_with('<') || Some(f) == fe.pp.predefines_file() {
                continue;
            }
            let p = snippet_path(name);
            if seen.insert(p.clone()) {
                files.push((p, sm.text(f).to_owned()));
            }
        }
        for c in fe.consulted {
            let p = snippet_path(&c.path);
            if !seen.contains(&p)
                && let Ok(t) = std::fs::read_to_string(&c.path)
            {
                seen.insert(p.clone());
                files.push((p, t));
            }
        }
        let predefines = fe.pp.predefines_file().map(|f| sm.text(f).to_owned()).unwrap_or_default();
        let dirs: Vec<String> = fe
            .lookups
            .map(|l| l.list.iter().map(|&d| snippet_path(&l.dirs[d as usize])).collect())
            .unwrap_or_default();
        let main = fe.main_file.map(|f| snippet_path(sm.file_name(f)));
        (files, predefines, dirs, main)
    })
    .map_err(|e| e.to_string())?;
    let (mut files, predefines, mut dirs, main) = report.value.ok_or("no parse")?;
    let main = main.ok_or("no main file")?;
    dirs.dedup();
    // The predefines first, from a header beside the main file.
    let pre = format!("{}/kiln-predefines.h", main.rsplit_once('/').map_or("", |(d, _)| d));
    for (p, t) in &mut files {
        if *p == main {
            *t = format!("#include \"kiln-predefines.h\"\n{t}");
        }
    }
    files.push((pre, predefines));
    // The language arguments: what the snippet takes of the command line.
    let mut std = "c++17".to_owned();
    let mut args: Vec<String> = Vec::new();
    for a in &line[1..] {
        if let Some(v) = a.strip_prefix("-std=") {
            v.clone_into(&mut std);
        } else if a.starts_with('-') && !a.starts_with("-D") && !a.starts_with("-I") {
            args.push(a.clone());
        }
    }
    let mut src = Source { files, main, include: dirs, std, args, target: None };
    // Drop what the snippet refuses, one argument at a time.
    for _ in 0..64 {
        let refused = src.with(|s, e| snippet::preprocess(s, e, |_| ()).err());
        let Some(e) = refused else { break };
        let arg = e.split('`').nth(1).map(str::to_owned).ok_or(e.clone())?;
        let before = src.args.len();
        src.args.retain(|a| *a != arg);
        if src.args.len() == before {
            return Err(e);
        }
    }
    Ok(src)
}

/// Test 4 for every call expression of the LLVM units named by
/// `SLIP_WP084_LLVM` (comma-separated file suffixes, e.g.
/// `lib/Support/StringRef.cpp`) of the corpus `$SLIP_CORPUS_DIR/llvm`,
/// as snippets ([`llvm_source`]). By hand: `cargo test -p slip_kiln
/// --test ranges -- --ignored --nocapture llvm`.
#[test]
#[ignore = "by hand: needs the llvm corpus and SLIP_WP084_LLVM"]
fn s5_4_test4_property_over_llvm_units() {
    let (Some(units), Some(root)) =
        (std::env::var_os("SLIP_WP084_LLVM"), std::env::var_os("SLIP_CORPUS_DIR"))
    else {
        eprintln!("SKIP: SLIP_WP084_LLVM and SLIP_CORPUS_DIR are not both set");
        return;
    };
    let corpus = PathBuf::from(root).join("llvm");
    let mut all = Tally::default();
    for unit in units.to_string_lossy().split(',') {
        let src = llvm_source(&corpus, unit).unwrap_or_else(|e| panic!("{unit}: {e}"));
        let calls = |k: ExprKind| {
            matches!(
                k,
                ExprKind::CallExpr | ExprKind::CXXMemberCallExpr | ExprKind::CXXOperatorCallExpr
            )
        };
        let tally = property_of(&src, unit, Some(calls), test_threads());
        eprintln!("{unit}: {} files; call expressions:", src.files.len());
        eprint!("{}", tally.report());
        for f in tally.failures.iter().take(20) {
            eprintln!("  {f}");
        }
        all.add(tally);
    }
    assert!(all.failures.is_empty(), "{}", all.report());
}
