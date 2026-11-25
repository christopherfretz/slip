//! `slip_kiln::sites` and `slip_kiln::tree` (work package WP-068,
//! `spec/landed/edits/WP-068.md`): where a name token is written and why
//! it cannot be edited (`spec/edits.md` §2.2, §2.3, §4.2).
//!
//! Every unit is a [`Tree`] under the work root (`common::work_root()`), parsed by the
//! production parse with Kiln's written-reference and declaration-extent
//! tables on, and owned but for `dep/` (`spec/harness.md` §6.2). Names are
//! found by where their first character is spelled; every answer is
//! rendered with the bytes of its ranges, and every `At` is checked
//! against `ranges::file_range_of_span` (the package's last test
//! obligation) in [`describe`].

mod common;

use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::ids::ExprId;
use slip_kiln::kiln::source::{FileId, Span};
use slip_kiln::ranges::{Tier, file_range_of_span};
use slip_kiln::sites::{Located, Reason, Sites};
use slip_kiln::snippet::{self, Extra, Snippet};
use slip_kiln::tree::Tree;
use slip_kiln::written::{decl_names, written_refs};
use slip_kiln::{ParseOptions, Unit};
use slip_model::edits::tiers::{T0, T1, T2, T3, X};
use slip_model::transform::{FileClass, Ownership};
use slip_model::{FileRef, Status};

// ---------------------------------------------------------------------
// Harness.

/// An answer of `name_token`, with paths relative to the tree and the
/// bytes of each range.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Got {
    At {
        file: String,
        lo: u64,
        text: String,
        tier: u64,
        site: Option<(u64, String)>,
    },
    Refused {
        reason: Reason,
        tier: u64,
        at: Option<(String, u64, String)>,
        site: Option<(u64, String)>,
    },
}

fn at(file: &str, lo: usize, text: &str, tier: u64, site: Option<(usize, &str)>) -> Got {
    Got::At {
        file: file.into(),
        lo: lo as u64,
        text: text.into(),
        tier,
        site: site.map(|(o, n)| (o as u64, n.into())),
    }
}

fn refused(
    reason: Reason,
    tier: u64,
    at: Option<(&str, usize, &str)>,
    site: Option<(usize, &str)>,
) -> Got {
    Got::Refused {
        reason,
        tier,
        at: at.map(|(f, lo, t)| (f.into(), lo as u64, t.into())),
        site: site.map(|(o, n)| (o as u64, n.into())),
    }
}

/// The tree's path of `file`, or the buffer's name.
fn path_of(u: &Unit<'_>, root: &str, file: FileId) -> String {
    match u.real_path(file) {
        Some(p) => {
            let p = String::from_utf8_lossy(p).into_owned();
            p.strip_prefix(root).map_or(p.clone(), |r| r.trim_start_matches('/').to_owned())
        }
        None => u.sm().file_name(file).to_owned(),
    }
}

fn text(u: &Unit<'_>, file: FileId, lo: u64, hi: u64) -> String {
    String::from_utf8_lossy(&u.sm().text(file).as_bytes()[lo as usize..hi as usize]).into_owned()
}

/// `name_token`'s answer, rendered; an `At` must be `file_range_of_span`'s.
fn describe(u: &Unit<'_>, sites: &Sites<'_, '_>, root: &str, span: Span, spelling: &str) -> Got {
    let site = |s: &Option<slip_model::edits::MacroSite>| {
        s.as_ref().map(|m| (m.name_start, String::from_utf8_lossy(&m.name).into_owned()))
    };
    match sites.name_token(span, spelling.as_bytes()) {
        Located::At { file, lo, hi, tier, macro_site } => {
            let r = file_range_of_span(u.sm(), span).expect("an At is a range of `ranges`");
            let rt = match r.tier {
                Tier::File => T0,
                Tier::MacroArgument => T1,
            };
            assert_eq!((r.file, u64::from(r.lo), u64::from(r.hi), rt), (file, lo, hi, tier));
            Got::At {
                file: path_of(u, root, file),
                lo,
                text: text(u, file, lo, hi),
                tier,
                site: site(&macro_site),
            }
        }
        Located::Refused { reason, tier, at, macro_site } => Got::Refused {
            reason,
            tier,
            at: at.map(|(f, lo, hi)| (path_of(u, root, f), lo, text(u, f, lo, hi))),
            site: site(&macro_site),
        },
    }
}

/// Every name span of the unit: written references and declaration names.
fn names(u: &Unit<'_>) -> Vec<Span> {
    let mut out: Vec<Span> = written_refs(u).iter().map(|r| r.name).collect();
    out.extend(decl_names(u).iter().map(|d| d.name));
    out
}

/// The name spans whose first character is spelled at byte `lo` of the
/// tree's file `file`.
fn spelled_at(u: &Unit<'_>, root: &str, file: &str, lo: usize) -> Vec<Span> {
    let sm = u.sm();
    let mut out: Vec<Span> = names(u)
        .into_iter()
        .filter(|s| {
            sm.offset_of(sm.spelling_loc(s.lo))
                .is_some_and(|(f, o)| o as usize == lo && path_of(u, root, f) == file)
        })
        .collect();
    out.sort_by_key(|s| (s.lo.to_raw(), s.hi.to_raw()));
    out.dedup();
    out
}

/// The byte offset of the first whole word `word` inside the one
/// occurrence of `ctx` in `src`.
fn off(src: &str, ctx: &str, word: &str) -> usize {
    let c = src.find(ctx).unwrap_or_else(|| panic!("`{ctx}` not in the source"));
    assert!(src[c + 1..].find(ctx).is_none(), "`{ctx}` twice in the source");
    let id = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = ctx.as_bytes();
    let w = ctx
        .match_indices(word)
        .map(|(i, _)| i)
        .find(|&i| {
            !i.checked_sub(1).is_some_and(|j| id(bytes[j]))
                && !bytes.get(i + word.len()).is_some_and(|&b| id(b))
        })
        .unwrap_or_else(|| panic!("`{word}` not a word of `{ctx}`"));
    c + w
}

fn written() -> ParseOptions<'static> {
    let mut o = ParseOptions::default();
    o.written = true;
    o
}

/// Parses `main` of a tree of `files` (owned, but for `dep/`, read-only,
/// and `gen/`, rebuilt) with
/// `args`, expects no error, and calls `f` with the unit, its sites and
/// the root.
fn with_tree<R: Send>(
    name: &str,
    files: &[(&str, &[u8])],
    args: &[&str],
    f: impl FnOnce(&Unit<'_>, &Sites<'_, '_>, &str) -> R + Send,
) -> R {
    let tree = Tree::new(&common::work_root(), name, files).unwrap();
    let root = tree.root().to_str().unwrap().to_owned();
    let ownership = Ownership {
        owned: vec![root.clone().into_bytes()],
        readonly: vec![format!("{root}/dep").into_bytes()],
        // WP-118: `gen/` is rebuilt in a tree that has it.
        rebuilt: if files.iter().any(|(p, _)| p.starts_with("gen/")) {
            vec![format!("{root}/gen").into_bytes()]
        } else {
            Vec::new()
        },
    };
    let obs = tree.observer("main.cpp", "c++20", args);
    let (shard, r) = tree
        .parse(&obs, &written(), |u| {
            let sites = Sites::new(u, &ownership);
            f(u, &sites, &root)
        })
        .unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{name}: {:?}", shard.outcome);
    r.expect("a parse happened")
}

/// The answers for every name whose first character is spelled at the
/// word `word` of `ctx` in `main.cpp`.
fn main_names(name: &str, src: &str, args: &[&str], ctx: &str, word: &str) -> Vec<Got> {
    let lo = off(src, ctx, word);
    with_tree(name, &[("main.cpp", src.as_bytes())], args, |u, sites, root| {
        let spans = spelled_at(u, root, "main.cpp", lo);
        assert!(!spans.is_empty(), "{name}: no name spelled at {lo}");
        spans.into_iter().map(|s| describe(u, sites, root, s, word)).collect()
    })
}

fn one(gots: Vec<Got>) -> Got {
    assert_eq!(gots.len(), 1, "one name: {gots:?}");
    gots.into_iter().next().unwrap()
}

// ---------------------------------------------------------------------
// §2.2: tiers.

#[test]
fn s2_2_tier0_plain_text() {
    let src = "int foo;\nint g() { return foo; }\n";
    let got = main_names("t0", src, &[], "return foo", "foo");
    assert_eq!(one(got), at("main.cpp", off(src, "return foo", "foo"), "foo", T0, None));
}

#[test]
fn s2_2_tier1_an_argument() {
    let src = "#define F(x) int x = 0;\nF(foo)\n";
    let got = one(main_names("t1", src, &[], "F(foo)", "foo"));
    assert_eq!(
        got,
        at("main.cpp", off(src, "F(foo)", "foo"), "foo", T1, Some((off(src, "F(foo)", "F"), "F")))
    );
}

#[test]
fn s2_2_tier1_through_two_macros() {
    let src = "#define F(x) int x = 0;\n#define G(y) F(y)\nG(foo)\n";
    let got = one(main_names("t1two", src, &[], "G(foo)", "foo"));
    assert_eq!(
        got,
        at("main.cpp", off(src, "G(foo)", "foo"), "foo", T1, Some((off(src, "G(foo)", "G"), "G")))
    );
}

// An argument that names an object-like macro whose body holds the
// token: tier 2, and the refusal points at the body.
#[test]
fn s2_2_tier2_an_argument_naming_an_object_like_macro() {
    let src = "#define N foo\n#define F(x) int x = 0;\nF(N)\n";
    let got = one(main_names("t2arg", src, &[], "N foo", "foo"));
    let lo = off(src, "N foo", "foo");
    assert_eq!(
        got,
        refused(
            Reason::MacroBody,
            T2,
            Some(("main.cpp", lo, "foo")),
            Some((off(src, "F(N)", "F"), "F"))
        )
    );
}

#[test]
fn s2_2_tier2_a_body() {
    let src = "#define D int foo = 0;\nD\n";
    let got = one(main_names("t2body", src, &[], "int foo", "foo"));
    let lo = off(src, "int foo", "foo");
    assert_eq!(
        got,
        refused(
            Reason::MacroBody,
            T2,
            Some(("main.cpp", lo, "foo")),
            Some((off(src, "\nD\n", "D"), "D"))
        )
    );
}

// A body in a read-only header: the refusal still points at it, and the
// invocation is in another file, so there is no `MacroSite` (its offset
// would be the claim's file's).
#[test]
fn s2_2_tier2_a_body_in_a_read_only_header() {
    let h = "#define D int foo = 0;\n";
    let src = "#include \"dep/d.h\"\nD\n";
    let got = with_tree(
        "t2dep",
        &[("main.cpp", src.as_bytes()), ("dep/d.h", h.as_bytes())],
        &[],
        |u, s, root| {
            let spans = spelled_at(u, root, "dep/d.h", off(h, "int foo", "foo"));
            spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
        },
    );
    assert_eq!(
        one(got),
        refused(Reason::MacroBody, T2, Some(("dep/d.h", off(h, "int foo", "foo"), "foo")), None)
    );
}

#[test]
fn s2_2_tier1_va_args_and_tier2_va_opt() {
    let src = "#define V(...) int __VA_ARGS__ = 0;\nV(foo)\n#define O(x, ...) int x __VA_OPT__(, bar) = 0;\nO(baz, 1)\n";
    let got = one(main_names("va", src, &[], "V(foo)", "foo"));
    assert_eq!(
        got,
        at("main.cpp", off(src, "V(foo)", "foo"), "foo", T1, Some((off(src, "V(foo)", "V"), "V")))
    );
    let got = one(main_names("vaopt", src, &[], ", bar)", "bar"));
    let lo = off(src, ", bar)", "bar");
    assert_eq!(
        got,
        refused(
            Reason::MacroBody,
            T2,
            Some(("main.cpp", lo, "bar")),
            Some((off(src, "O(baz", "O"), "O"))
        )
    );
}

// Tier 3: a pasted name; the refusal points at the invocation's name.
#[test]
fn s2_2_tier3_pasted() {
    let src = "#define P(a, b) int a##b = 0;\nP(fo, o)\nint g() { return foo; }\n";
    let lo = off(src, "P(fo", "P");
    let got = with_tree("t3", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        // The declaration's name, found by its expansion location.
        let sm = u.sm();
        let spans: Vec<Span> = decl_names(u)
            .iter()
            .map(|d| d.name)
            .filter(|n| sm.offset_of(sm.expansion_loc(n.lo)).is_some_and(|(_, o)| o as usize == lo))
            .collect();
        spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
    });
    assert_eq!(one(got), refused(Reason::Pasted, T3, Some(("main.cpp", lo, "P")), Some((lo, "P"))));
}

// Tier 3: a stringised argument (the span of the string literal).
#[test]
fn s2_2_tier3_stringised() {
    let src = "#define S(x) #x\nconst char *s = S(foo);\n";
    let lo = off(src, "S(foo)", "S");
    let got = with_tree("t3str", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let ast = u.ast();
        let lits: Vec<Span> = (1..=ast.exprs.len())
            .filter_map(|k| ExprId::try_new(u32::try_from(k).ok()?))
            .filter(|&e| ast.exprs.get(e).kind() == ExprKind::StringLiteral)
            .map(|e| {
                let b = ast.begin_loc(e);
                Span::new(b, u.token_end(b).unwrap())
            })
            .collect();
        lits.into_iter().map(|sp| describe(u, s, root, sp, "\"foo\"")).collect::<Vec<_>>()
    });
    assert_eq!(one(got), refused(Reason::Pasted, T3, Some(("main.cpp", lo, "S")), Some((lo, "S"))));
}

// Tier X: a name from `-D` is written in the predefines buffer.
#[test]
fn s2_2_tierx_a_name_from_the_command_line() {
    let src = "int NAME = 0;\n";
    let lo = off(src, "NAME", "NAME");
    let got = with_tree("tx", &[("main.cpp", src.as_bytes())], &["-DNAME=foo"], |u, s, root| {
        let sm = u.sm();
        let spans: Vec<Span> = decl_names(u)
            .iter()
            .map(|d| d.name)
            .filter(|n| sm.offset_of(sm.expansion_loc(n.lo)).is_some_and(|(_, o)| o as usize == lo))
            .collect();
        spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
    });
    assert_eq!(
        one(got),
        refused(Reason::NotAFile, X, Some(("main.cpp", lo, "NAME")), Some((lo, "NAME")))
    );
}

// Tier X: a qualified name is more than one token, and `name_token`
// answers for one (finding F1 of the review): the span from the
// qualifier to the name is `crossing`, here also because its qualifier
// and last component come from different expansions (two arguments) and
// Kiln gives no range for it. The last component alone is tier 1.
#[test]
fn s2_2_tierx_qualifier_and_name_from_different_expansions() {
    let src = "namespace n { int m; }\n#define F(a, b) int y = a::b + 0;\nF(n, m)\n";
    let lo = off(src, "F(n, m)", "F");
    let got = with_tree("txq", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let sm = u.sm();
        let r = written_refs(u)
            .into_iter()
            .find(|r| {
                sm.offset_of(sm.spelling_loc(r.name.lo)).is_some_and(|(_, o)| o as usize == lo + 5)
            })
            .expect("the reference to m");
        assert_ne!(r.first, r.name.lo, "qualified");
        (
            describe(u, s, root, Span::new(r.first, r.name.hi), "n::m"),
            describe(u, s, root, r.name, "m"),
        )
    });
    assert_eq!(got.0, refused(Reason::Crossing, X, Some(("main.cpp", lo, "F")), Some((lo, "F"))));
    assert_eq!(got.1, at("main.cpp", lo + 5, "m", T1, Some((lo, "F"))));
}

/// The answers for the written references whose name's first character
/// is written at the word `word` of `ctx` in `main.cpp`, for the span
/// from the start of the whole written name (`first`) to the name's end,
/// with `spelling`.
fn qualified(name: &str, src: &str, ctx: &str, word: &str, spelling: &str) -> Vec<Got> {
    let lo = off(src, ctx, word);
    with_tree(name, &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let sm = u.sm();
        written_refs(u)
            .into_iter()
            .filter(|r| {
                sm.offset_of(sm.expansion_loc(r.first)).is_some_and(|(_, o)| o as usize == lo)
            })
            .filter(|r| r.first != r.name.lo)
            .map(|r| describe(u, s, root, Span::new(r.first, r.name.hi), spelling))
            .collect()
    })
}

// F1 of the review: `n::m` with `#define m m`. Kiln's range of the span
// is the bytes `n::m` (its two ends are file locations), but `m` is
// written in a replacement list. The span is not one token: `crossing`.
// The token `m` alone is `macro_body`.
#[test]
fn s2_3_crossing_a_qualified_name_ending_in_a_macro() {
    let src = "namespace n { int m; }\n#define m m\nint g() { return n::m; }\n";
    let got = qualified("f1", src, "return n::m", "n", "n::m");
    assert_eq!(one(got), refused(Reason::Crossing, X, None, None));
    let use_ = off(src, "return n::m", "m");
    let got = with_tree("f1m", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let sm = u.sm();
        let names: Vec<Span> = written_refs(u)
            .iter()
            .map(|r| r.name)
            .filter(|n| {
                sm.offset_of(sm.expansion_loc(n.lo)).is_some_and(|(_, o)| o as usize == use_)
            })
            .collect();
        names.into_iter().map(|n| describe(u, s, root, n, "m")).collect::<Vec<_>>()
    });
    let got = one(got);
    assert_eq!(
        got,
        refused(
            Reason::MacroBody,
            T2,
            Some(("main.cpp", off(src, "m m", "m") + 2, "m")),
            Some((use_, "m"))
        )
    );
}

// F1 of the review: the macro in the middle, `a::B::c` with `#define B B`.
#[test]
fn s2_3_crossing_a_qualified_name_through_a_macro() {
    let src = "namespace a { namespace B { int c; } }\n#define B B\nint g() { return a::B::c; }\n";
    let got = qualified("f1b", src, "return a::B::c", "a", "a::B::c");
    // Two references start at `a`: `B` qualified by `a`, and `c`.
    assert_eq!(got, vec![refused(Reason::Crossing, X, None, None); got.len()]);
    assert!(!got.is_empty());
}

// ---------------------------------------------------------------------
// What `slip_kiln::written` gives as the name of each form of name
// (`spec/landed/edits/WP-068.md`, "Name forms"), and the answer for it.

/// Names, each with its span's text and the answer for that text.
type Named = Vec<(String, Got)>;

/// The declarations and the references of `main.cpp` whose name's span
/// starts at the word `word` of `ctx`.
fn forms(name: &str, src: &str, ctx: &str, word: &str) -> (Named, Named) {
    let lo = off(src, ctx, word);
    with_tree(name, &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let sm = u.sm();
        let at_lo = |sp: &Span| sm.offset_of(sp.lo).is_some_and(|(_, o)| o as usize == lo);
        let answer = |sp: Span| {
            let t = sm.span_text(sp).unwrap().to_owned();
            let g = describe(u, s, root, sp, &t);
            (t, g)
        };
        let decls = decl_names(u).iter().map(|d| d.name).filter(at_lo).map(answer).collect();
        let refs = written_refs(u).iter().map(|r| r.name).filter(at_lo).map(answer).collect();
        (decls, refs)
    })
}

// A redeclaration's declarator-id is also a written reference, of role
// `Redecl`, to the entity it redeclares (Kiln WP-1932 stage 3): the
// definition `S::~S`, `void n::f`, the explicit specialisation `f<int>`
// below give the same name twice, one answer each.
#[test]
fn names_of_a_destructor() {
    let src = "struct S { ~S(); };\nS::~S() {}\nvoid g(S *p) { p->~S(); }\n";
    for ctx in ["{ ~S(); }", "~S() {}", "p->~S()"] {
        let lo = off(src, ctx, "S");
        let (decls, refs) = forms("dtor", src, ctx, "S");
        let want = vec![("S".to_owned(), at("main.cpp", lo, "S", T0, None))];
        match ctx {
            "p->~S()" => assert_eq!((decls, refs), (vec![], want), "{ctx}"),
            "~S() {}" => assert_eq!((decls, refs), (want.clone(), want), "{ctx}"),
            _ => assert_eq!((decls, refs), (want, vec![]), "{ctx}"),
        }
    }
}

#[test]
fn names_of_an_operator_function() {
    let src = "struct S { S operator+(S) const; };\nS S::operator+(S) const { return *this; }\n\
               S g(S a) { return a.operator+(a); }\nS h(S a) { return a + a; }\n";
    let (decls, refs) = forms("op", src, "operator+(S) const;", "operator");
    assert_eq!((decls, refs), (vec![], vec![]), "a declaration of an operator has no name");
    let (decls, refs) = forms("op", src, "a.operator+(a)", "operator");
    let crossing = refused(Reason::Crossing, X, None, None);
    assert_eq!((decls, refs), (vec![], vec![("operator+".to_owned(), crossing)]));
}

#[test]
fn names_of_a_conversion_function() {
    let src = "struct S { operator int() const; };\nS::operator int() const { return 0; }\n\
               int g(S a) { return a.operator int(); }\n";
    let (decls, refs) = forms("conv", src, "operator int() const;", "operator");
    assert_eq!((decls, refs), (vec![], vec![]));
    let (decls, refs) = forms("conv", src, "a.operator int()", "operator");
    let crossing = refused(Reason::Crossing, X, None, None);
    assert_eq!((decls, refs), (vec![], vec![("operator int".to_owned(), crossing)]));
}

#[test]
fn names_of_a_literal_operator() {
    let src = "unsigned long long operator\"\"_k(unsigned long long v) { return v; }\n\
               unsigned long long x = operator\"\"_k(1);\nunsigned long long y = 12_k;\n";
    let (decls, refs) = forms("lit", src, "operator\"\"_k(unsigned", "operator");
    assert_eq!((decls, refs), (vec![], vec![]));
    let (decls, refs) = forms("lit", src, "operator\"\"_k(1)", "operator");
    let crossing = refused(Reason::Crossing, X, None, None);
    assert_eq!((decls, refs), (vec![], vec![("operator\"\"_k".to_owned(), crossing)]));
    let (decls, refs) = forms("lit", src, "12_k", "12_k");
    assert_eq!((decls, refs), (vec![], vec![]), "a use as a literal is no name");
}

#[test]
fn names_of_a_template_id() {
    let src = "template <class T> void f(T) {}\ntemplate <> void f<int>(int) {}\n\
               void g() { f<int>(1); }\n";
    for (ctx, decl) in [("f<int>(int)", true), ("f<int>(1)", false)] {
        let lo = off(src, ctx, "f");
        let (decls, refs) = forms("tid", src, ctx, "f");
        let want = vec![("f".to_owned(), at("main.cpp", lo, "f", T0, None))];
        if decl {
            assert_eq!((decls, refs), (want.clone(), want), "{ctx}: a redeclaration");
        } else {
            assert_eq!((decls, refs), (vec![], want), "{ctx}");
        }
    }
}

#[test]
fn names_of_a_qualified_declarator() {
    let src = "namespace n { void f(); }\nvoid n::f() {}\nvoid g() { n::f(); }\n";
    for (ctx, decl) in [("::f() {}", true), ("g() { n::f", false)] {
        let lo = off(src, ctx, "f");
        let (decls, refs) = forms("qual", src, ctx, "f");
        let want = vec![("f".to_owned(), at("main.cpp", lo, "f", T0, None))];
        if decl {
            assert_eq!((decls, refs), (want.clone(), want), "{ctx}: a redeclaration");
        } else {
            assert_eq!((decls, refs), (vec![], want), "{ctx}");
        }
    }
    // The whole written name of the reference, `n::f`, is two tokens.
    let got = qualified("qualn", src, "n::f(); }", "n", "n::f");
    assert_eq!(one(got), refused(Reason::Crossing, X, None, None));
}

// ---------------------------------------------------------------------
// §2.3: each reason, and the range of each kind of refusal.

#[test]
fn s2_3_directive_interim_raw_string_line() {
    // INTERIM (`spec/landed/api/WP-084.md` decision 10): the line holding
    // `foo` begins with `#` inside a raw string, so `ranges` gives no
    // range and says why. Tier 0, no invocation: `at` is `None`.
    let src = "const char *s = R\"(\n# )\"; int foo = 0;\n";
    let got = one(main_names("dir", src, &[], "int foo", "foo"));
    assert_eq!(got, refused(Reason::Directive, T0, None, None));
}

#[test]
fn s2_3_not_owned() {
    let h = "int foo = 0;\n";
    let src = "#include \"dep/h.h\"\nint g() { return foo; }\n";
    let got = with_tree(
        "ro",
        &[("main.cpp", src.as_bytes()), ("dep/h.h", h.as_bytes())],
        &[],
        |u, s, root| {
            let spans = spelled_at(u, root, "dep/h.h", off(h, "foo", "foo"));
            spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
        },
    );
    assert_eq!(one(got), refused(Reason::NotOwned, T0, Some(("dep/h.h", 4, "foo")), None));
}

// WP-118 R7: a name in a rebuilt file is refused `rebuilt`, not
// `not_owned`; a read-only file beside it stays `not_owned` (M21), and
// `Sites::class` gives each file's class.
#[test]
fn s2_3_rebuilt() {
    let h = "int foo = 0;\n";
    let src = "#include \"gen/h.h\"\n#include \"dep/r.h\"\nint g() { return foo + bar; }\n";
    let r = "int bar = 0;\n";
    let got = with_tree(
        "rebuilt",
        &[("main.cpp", src.as_bytes()), ("gen/h.h", h.as_bytes()), ("dep/r.h", r.as_bytes())],
        &[],
        |u, s, root| {
            let class = |name: &str| {
                let f = u
                    .sm()
                    .file_ids()
                    .find(|&f| path_of(u, root, f) == name)
                    .unwrap_or_else(|| panic!("no {name}"));
                (s.is_owned(f), s.class(f))
            };
            let classes = [class("main.cpp"), class("gen/h.h"), class("dep/r.h")];
            let mut got = Vec::new();
            for (file, text, word) in [("gen/h.h", h, "foo"), ("dep/r.h", r, "bar")] {
                let spans = spelled_at(u, root, file, off(text, word, word));
                got.extend(spans.into_iter().map(|sp| describe(u, s, root, sp, word)));
            }
            (classes, got)
        },
    );
    assert_eq!(
        got.0,
        [(true, FileClass::Owned), (false, FileClass::Rebuilt), (false, FileClass::ReadOnly)]
    );
    assert_eq!(
        got.1,
        vec![
            refused(Reason::Rebuilt, T0, Some(("gen/h.h", 4, "foo")), None),
            refused(Reason::NotOwned, T0, Some(("dep/r.h", 4, "bar")), None),
        ]
    );
}

// A file with an invalid byte before the token: Kiln's offsets are its
// text's, so no `at` is given in that file.
#[test]
fn s2_3_file_recoded() {
    let src: &[u8] = b"// \xff\nint foo = 0;\n";
    let got = with_tree("rec", &[("main.cpp", src)], &[], |u, s, root| {
        // Kiln's text has three bytes for the one invalid byte.
        let spans = spelled_at(u, root, "main.cpp", 3 + 3 + 1 + 4);
        spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
    });
    assert_eq!(one(got), refused(Reason::FileRecoded, T0, None, None));
}

// `é` written as a universal-character-name, in the declaration and in a
// reference: the bytes are not the name.
#[test]
fn s2_3_not_plain_a_universal_character_name() {
    let src = "int caf\\u00e9 = 0;\nint g() { return caf\\u00e9; }\n";
    for ctx in ["int caf", "return caf"] {
        let lo = off(src, ctx, "caf");
        let got = with_tree("ucn", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
            let spans = spelled_at(u, root, "main.cpp", lo);
            spans.into_iter().map(|sp| describe(u, s, root, sp, "café")).collect::<Vec<_>>()
        });
        assert_eq!(
            one(got),
            refused(Reason::NotPlain, T0, Some(("main.cpp", lo, "caf\\u00e9")), None),
            "{ctx}"
        );
    }
}

// WP-067's correction (`spec/landed/edits/WP-067.md`, "Corrections"): a
// declaration whose name is split by a line splice, or a class named with
// a universal-character-name, is a declaration name; `name_token`
// refuses it `not_plain`, a refusal the user sees, not a silent skip.
#[test]
fn s2_3_not_plain_a_declaration_split_by_a_splice() {
    let src = "int fo\\\no = 0;\nstruct S\\u00e9 {};\nint g() { return fo\\\no; }\n";
    let got = one(main_names("splice", src, &[], "int fo", "fo"));
    assert_eq!(got, refused(Reason::NotPlain, T0, Some(("main.cpp", 4, "fo\\\no")), None));
    let lo = off(src, "struct S", "S");
    let got = with_tree("sucn", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let spans = spelled_at(u, root, "main.cpp", lo);
        spans.into_iter().map(|sp| describe(u, s, root, sp, "Sé")).collect::<Vec<_>>()
    });
    assert_eq!(one(got), refused(Reason::NotPlain, T0, Some(("main.cpp", lo, "S\\u00e9")), None));
}

// A name that is a whole expansion: `ID(foo)` is tier 1, and Kiln's
// range is the invocation (tier 0): `not_plain`, at the invocation.
#[test]
fn s2_3_not_plain_a_whole_expansion() {
    let src = "#define ID(x) x\nint foo = 0;\nint g() { return ID(foo); }\n";
    let got = one(main_names("id", src, &[], "ID(foo)", "foo"));
    let lo = off(src, "ID(foo)", "ID");
    assert_eq!(
        got,
        refused(Reason::NotPlain, T1, Some(("main.cpp", lo, "ID(foo)")), Some((lo, "ID")))
    );
}

// `NAME` defined as `foo`, and `foo` defined as `foo`: written in a
// replacement list, whatever Kiln's range is.
#[test]
fn s2_3_macro_body_whole_expansions() {
    let src = "#define NAME foo\nint NAME = 0;\n";
    let got = one(main_names("name", src, &[], "NAME foo", "foo"));
    let lo = off(src, "NAME foo", "foo");
    let inv = off(src, "int NAME", "NAME");
    assert_eq!(
        got,
        refused(Reason::MacroBody, T2, Some(("main.cpp", lo, "foo")), Some((inv, "NAME")))
    );
    let src = "#define foo foo\nint foo = 0;\n";
    let lo = off(src, "foo foo", "foo") + 4;
    let got = with_tree("foofoo", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let spans = spelled_at(u, root, "main.cpp", lo);
        spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
    });
    let inv = off(src, "int foo", "foo");
    assert_eq!(
        one(got),
        refused(Reason::MacroBody, T2, Some(("main.cpp", lo, "foo")), Some((inv, "foo")))
    );
}

// ---------------------------------------------------------------------
// §2.3: two reasons at once, for each adjacent pair of the order.

// Tier (rule 1) before Kiln's range (rule 2): a body token Kiln gives no
// range for is `macro_body`, a pasted one `pasted`; both tested above
// (`s2_2_tier2_a_body`: `foo` is inside `D`'s expansion, which Kiln's
// rules do not map; `s2_2_tier3_pasted`).

/// The answers for the names spelled at byte `lo` of a snippet's `t.cpp`,
/// with an ownership that owns `/`.
fn in_snippet(
    src: &str,
    lo: usize,
    spelling: &str,
    span_of: fn(&Unit<'_>, Span) -> Span,
) -> Vec<Got> {
    let files = [("t.cpp", src)];
    let snippet = Snippet { files: &files, main: "t.cpp", include: &[""], std: "c++20" };
    let ownership =
        Ownership { owned: vec![b"/".to_vec()], readonly: Vec::new(), rebuilt: Vec::new() };
    snippet::parse_with_tables(&snippet, &Extra::default(), |p| {
        let u = p.unit();
        let sites = Sites::new(u, &ownership);
        let sm = u.sm();
        let spans: Vec<Span> = names(u)
            .into_iter()
            .filter(|s| sm.offset_of(sm.spelling_loc(s.lo)).is_some_and(|(_, o)| o as usize == lo))
            .collect();
        assert!(!spans.is_empty(), "no name at {lo}");
        spans.into_iter().map(|s| describe(u, &sites, "", span_of(u, s), spelling)).collect()
    })
    .unwrap()
}

// A snippet's buffer is not a file: `not_a_file`, also where the file is
// owned (rules 3 and 4).
#[test]
fn s2_3_order_not_a_file_before_not_owned() {
    let src = "int foo = 0;\n";
    let got = one(in_snippet(src, 4, "foo", |_, s| s));
    assert_eq!(got, refused(Reason::NotAFile, X, None, None));
}

// `crossing` before `not_a_file` (one row of the table, in its order).
#[test]
fn s2_3_order_crossing_before_not_a_file() {
    let src = "namespace n { int m; }\n#define F(a, b) int y = a::b + 0;\nF(n, m)\n";
    let lo = off(src, "F(n, m)", "m");
    let got = in_snippet(src, lo, "n::m", |u, s| {
        let r = written_refs(u).into_iter().find(|r| r.name == s).unwrap();
        Span::new(r.first, s.hi)
    });
    assert_eq!(one(got), refused(Reason::Crossing, X, None, None));
}

// `not_a_file` before `directive`.
#[test]
fn s2_3_order_not_a_file_before_directive() {
    let src = "const char *s = R\"(\n# )\"; int foo = 0;\n";
    let got = one(in_snippet(src, off(src, "int foo", "foo"), "foo", |_, s| s));
    assert_eq!(got, refused(Reason::NotAFile, X, None, None));
}

// `directive` before `not_owned`.
#[test]
fn s2_3_order_directive_before_not_owned() {
    let h = "const char *s = R\"(\n# )\"; int foo = 0;\n";
    let src = "#include \"dep/h.h\"\n";
    let got = with_tree(
        "dirro",
        &[("main.cpp", src.as_bytes()), ("dep/h.h", h.as_bytes())],
        &[],
        |u, s, root| {
            let spans = spelled_at(u, root, "dep/h.h", off(h, "int foo", "foo"));
            spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
        },
    );
    assert_eq!(one(got), refused(Reason::Directive, T0, None, None));
}

// `not_owned` before `file_recoded`; no `at` in a recoded file.
#[test]
fn s2_3_order_not_owned_before_file_recoded() {
    let h: &[u8] = b"// \xff\nint foo = 0;\n";
    let src = "#include \"dep/h.h\"\n";
    let got =
        with_tree("rorec", &[("main.cpp", src.as_bytes()), ("dep/h.h", h)], &[], |u, s, root| {
            let spans = spelled_at(u, root, "dep/h.h", 3 + 3 + 1 + 4);
            spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
        });
    assert_eq!(one(got), refused(Reason::NotOwned, T0, None, None));
}

// `file_recoded` before `not_plain`.
#[test]
fn s2_3_order_file_recoded_before_not_plain() {
    let src: &[u8] = b"// \xff\nint caf\xc3\xa9 = 0; int g() { return caf\\u00e9; }\n";
    let got = with_tree("recucn", &[("main.cpp", src)], &[], |u, s, root| {
        // `return caf`'s offset in Kiln's text, two bytes past the file's.
        let spans = spelled_at(u, root, "main.cpp", 37 + 2);
        spans.into_iter().map(|sp| describe(u, s, root, sp, "café")).collect::<Vec<_>>()
    });
    assert_eq!(one(got), refused(Reason::FileRecoded, T0, None, None));
}

// Each reason's value on the wire (`spec/edits.md` §4.2), in the table's
// order.
#[test]
fn s2_3_reason_wire_values() {
    use slip_model::edits::Reason as W;
    let all = [
        (Reason::MacroBody, W::MacroBody),
        (Reason::Pasted, W::Pasted),
        (Reason::Crossing, W::Crossing),
        (Reason::NotAFile, W::NotAFile),
        (Reason::Directive, W::Directive),
        (Reason::NotOwned, W::NotOwned),
        (Reason::Rebuilt, W::Rebuilt),
        (Reason::FileRecoded, W::FileRecoded),
        (Reason::NotPlain, W::NotPlain),
    ];
    for (r, w) in all {
        assert_eq!(r.wire(), w, "{r:?}");
    }
    assert!(all.windows(2).all(|p| p[0].0 < p[1].0), "the order of the table");
}

// ---------------------------------------------------------------------
// §2.2: tiers are an observer's; votes are the transform's.

// The same bytes at tier 0 in one command and tier 1 in another: a name
// before `(` that is a macro under `-D`.
#[test]
fn s2_2_same_bytes_tier0_and_tier1() {
    let src = "void g(int);\nvoid k(int);\nint foo;\nvoid h() { g(foo); }\n";
    let lo = off(src, "g(foo)", "foo");
    let plain = one(main_names("cmd0", src, &[], "g(foo)", "foo"));
    assert_eq!(plain, at("main.cpp", lo, "foo", T0, None));
    let mac = one(main_names("cmd1", src, &["-Dg(x)=k(x)"], "g(foo)", "foo"));
    assert_eq!(mac, at("main.cpp", lo, "foo", T1, Some((off(src, "g(foo)", "g"), "g"))));
}

// An argument substituted twice gives two names with equal answers.
#[test]
fn s2_2_an_argument_substituted_twice() {
    let src = "int foo;\n#define TW(x) (x + x)\nint g() { return TW(foo); }\n";
    let got = main_names("twice", src, &[], "TW(foo)", "foo");
    let one = at(
        "main.cpp",
        off(src, "TW(foo)", "foo"),
        "foo",
        T1,
        Some((off(src, "TW(foo)", "TW"), "TW")),
    );
    assert_eq!(got, vec![one.clone(), one]);
}

// A byte-order mark is part of the text: offsets are the file's.
#[test]
fn s2_3_byte_order_mark() {
    let src: &[u8] = b"\xef\xbb\xbfint foo = 0;\n";
    let got = with_tree("bom", &[("main.cpp", src)], &[], |u, s, root| {
        let spans = spelled_at(u, root, "main.cpp", 7);
        spans.into_iter().map(|sp| describe(u, s, root, sp, "foo")).collect::<Vec<_>>()
    });
    assert_eq!(one(got), at("main.cpp", 7, "foo", T0, None));
}

// ---------------------------------------------------------------------
// `file_ref`, `is_owned`.

#[test]
fn file_ref_is_the_shard_input() {
    let h = "int foo = 0;\n";
    let src = "#include \"dep/h.h\"\nint bar = foo;\n";
    let tree = Tree::new(
        &common::work_root(),
        "fileref",
        &[("main.cpp", src.as_bytes()), ("dep/h.h", h.as_bytes())],
    )
    .unwrap();
    let root = tree.root().to_str().unwrap().to_owned();
    let ownership = Ownership {
        owned: vec![root.clone().into_bytes()],
        readonly: vec![format!("{root}/dep").into_bytes()],
        rebuilt: Vec::new(),
    };
    let obs = tree.observer("main.cpp", "c++20", &[]);
    let (shard, got) = tree
        .parse(&obs, &written(), |u| {
            let s = Sites::new(u, &ownership);
            let sm = u.sm();
            sm.file_ids()
                .map(|f| (path_of(u, &root, f), s.file_ref(f), s.is_owned(f)))
                .collect::<Vec<(String, Option<FileRef>, bool)>>()
        })
        .unwrap();
    let got = got.unwrap();
    let inputs = shard.inputs.unwrap();
    let main = got.iter().find(|g| g.0 == "main.cpp").unwrap();
    let dep = got.iter().find(|g| g.0 == "dep/h.h").unwrap();
    assert_eq!(main.1.as_ref(), inputs.iter().find(|i| i.path.ends_with(b"/main.cpp")));
    assert_eq!(dep.1.as_ref(), inputs.iter().find(|i| i.path.ends_with(b"/dep/h.h")));
    assert_eq!(
        main.1.as_ref().unwrap().path,
        obs.main.path,
        "the observer's main is the real path"
    );
    assert!(main.2 && !dep.2, "main owned, dep read-only");
    // Every buffer that is not a file (the predefines) has no FileRef.
    for g in &got {
        if g.0 != "main.cpp" && g.0 != "dep/h.h" {
            assert_eq!((&g.1, g.2), (&None, false), "{}", g.0);
        }
    }
    assert!(got.len() > 2, "the predefines are buffers too");
}

// ---------------------------------------------------------------------
// `slip_kiln::tree`.

#[test]
fn tree_directories_are_fresh_and_removed_on_drop() {
    let base = common::work_root().join("trees");
    let a =
        Tree::new(&base, "same", &[("a/b/c.h", b"x"), ("a/d.h", b"y"), ("m.cpp", b"")]).unwrap();
    let b = Tree::new(&base, "same", &[("m.cpp", b"")]).unwrap();
    assert_ne!(a.root(), b.root());
    assert_eq!(std::fs::read(a.root().join("a/b/c.h")).unwrap(), b"x");
    let (ra, rb) = (a.root().to_owned(), b.root().to_owned());
    drop(a);
    assert!(!ra.exists(), "removed on drop");
    assert!(rb.exists());
    drop(b);
    assert!(!rb.exists());
}

#[test]
fn tree_refuses_paths_outside_it() {
    let base = common::work_root().join("trees");
    for bad in ["/abs.h", "../up.h", "a/../b.h", "./a.h", ""] {
        let e = Tree::new(&base, "bad", &[(bad, b"")]).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
    }
    let e = Tree::new(&base, "twice", &[("a.h", b""), ("a.h", b"")]).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
}

// Review finding F7: a tree's name is one normal path component.
#[test]
fn tree_refuses_names_that_are_not_one_component() {
    let base = common::work_root().join("trees");
    for bad in ["", ".", "..", "a/b", "/abs", "a/", "../x", "./a"] {
        let e = Tree::new(&base, bad, &[("m.cpp", b"")]).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
    }
    assert!(Tree::new(&base, "a.b-c", &[("m.cpp", b"")]).is_ok());
}

#[test]
fn tree_observer_is_the_default_command() {
    let tree = Tree::new(&common::work_root(), "cmd", &[("m.cpp", b"int x;")]).unwrap();
    let root = tree.root().to_str().unwrap();
    let o = tree.observer("m.cpp", "c++23", &["-DX=1"]);
    let argv: Vec<String> =
        o.command.argv.iter().map(|a| String::from_utf8(a.clone()).unwrap()).collect();
    assert_eq!(
        argv,
        vec![
            "kiln".to_owned(),
            "-std=c++23".into(),
            "-DX=1".into(),
            format!("-I{root}"),
            format!("-I{root}/dep"),
            "-c".into(),
            "m.cpp".into()
        ]
    );
    assert_eq!(o.command.directory, root.as_bytes());
    assert_eq!(o.id, o.command.observer_id());
    assert_eq!((o.main.size, o.main.content), (6, slip_model::hash::content_digest(b"int x;")));
    let (shard, _) = tree.parse(&o, &ParseOptions::default(), |_| ()).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
}

// Rule 6 by tiers alone: the caller expects the invocation's bytes, which
// are `ranges`' tier 0 range, but the token is written at tier 1.
#[test]
fn s2_3_not_plain_by_tiers_alone() {
    let src = "#define ID(x) x\nint foo = 0;\nint g() { return ID(foo); }\n";
    let lo = off(src, "ID(foo)", "foo");
    let inv = off(src, "ID(foo)", "ID");
    let got = with_tree("idtier", &[("main.cpp", src.as_bytes())], &[], |u, s, root| {
        let spans = spelled_at(u, root, "main.cpp", lo);
        spans.into_iter().map(|sp| describe(u, s, root, sp, "ID(foo)")).collect::<Vec<_>>()
    });
    assert_eq!(
        one(got),
        refused(Reason::NotPlain, T1, Some(("main.cpp", inv, "ID(foo)")), Some((inv, "ID")))
    );
}

// F3 of the review: a paste whose other operand is empty makes nothing:
// the token is the argument's own, tier 1 (`spec/edits.md` §2.2).
#[test]
fn s2_2_tier1_a_paste_with_an_empty_right_operand() {
    let src = "#define CAT(a, b) int a##b = 0;\nCAT(foo, )\nint g() { return foo; }\n";
    let got = one(main_names("catr", src, &[], "CAT(foo, )", "foo"));
    let site = Some((off(src, "CAT(foo", "CAT"), "CAT"));
    assert_eq!(got, at("main.cpp", off(src, "CAT(foo, )", "foo"), "foo", T1, site));
}

#[test]
fn s2_2_tier1_a_paste_with_an_empty_left_operand() {
    let src = "#define CAT(a, b) int a##b = 0;\nCAT(, bar)\nint g() { return bar; }\n";
    let got = one(main_names("catl", src, &[], "CAT(, bar)", "bar"));
    let site = Some((off(src, "CAT(, bar)", "CAT"), "CAT"));
    assert_eq!(got, at("main.cpp", off(src, "CAT(, bar)", "bar"), "bar", T1, site));
}

// KF3 §3 edge rows 3, 5 and 6 (`docs/requests/KF3-location-utilities.md`).
#[test]
fn s2_2_kf3_edge_rows() {
    // Row 3: an argument inside another argument: tier 1. With `B` the
    // identity, `B(foo)` is a whole expansion and Kiln's range is it:
    // `not_plain`, as for `ID(foo)`.
    let src = "int foo;\n#define A(x) int v = x;\n#define B(y) (y)\nA(B(foo))\n";
    let got = one(main_names("row3", src, &[], "B(foo)", "foo"));
    assert_eq!(
        got,
        at("main.cpp", off(src, "B(foo)", "foo"), "foo", T1, Some((off(src, "A(B", "A"), "A")))
    );
    let src = "#define A(x) int x = 0;\n#define B(y) y\nA(B(foo))\n";
    let got = one(main_names("row3id", src, &[], "B(foo)", "foo"));
    let b = off(src, "B(foo)", "B");
    assert_eq!(
        got,
        refused(
            Reason::NotPlain,
            T1,
            Some(("main.cpp", b, "B(foo)")),
            Some((off(src, "A(B", "A"), "A"))
        )
    );
    // Row 5: an invocation whose name comes from a body and whose
    // argument is written in the file.
    let src = "#define F(x) int x = 0;\n#define FN F\nFN(foo)\n";
    let got = one(main_names("row5", src, &[], "FN(foo)", "foo"));
    assert_eq!(
        got,
        at(
            "main.cpp",
            off(src, "FN(foo)", "foo"),
            "foo",
            T1,
            Some((off(src, "FN(foo)", "FN"), "FN"))
        )
    );
    // Row 6: one argument both substituted and stringised: the name is
    // tier 1 (the string is `pasted`, `s2_2_tier3_stringised`). Editing
    // it changes the string too; that is as designed
    // (`docs/design/04-macros-and-configurations.md`, "Stringised
    // arguments"; `spec/edits.md` §2.2): verification reports the unit
    // that holds the string as changed, the verdict of pitfall rows N17
    // and R15 (`spec/pitfalls.md`), `mismatch changed`.
    let src = "#define SS(x) int x = 0; const char *s_##x = #x;\nSS(foo)\n";
    let got = one(main_names("row6", src, &[], "SS(foo)", "foo"));
    assert_eq!(
        got,
        at(
            "main.cpp",
            off(src, "SS(foo)", "foo"),
            "foo",
            T1,
            Some((off(src, "SS(foo)", "SS"), "SS"))
        )
    );
}

// spec/harness.md §11.5: this crate's work directories follow the rule.
#[test]
fn s11_5_work_root() {
    common::work_root::self_test(env!("CARGO_TARGET_TMPDIR"));
}
