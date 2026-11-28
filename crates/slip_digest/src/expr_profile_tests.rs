//! Tests of profile mode (`expr_profile.rs`).
//!
//! The two nets (WP-036 review, item 1b) compare the bytes of profile
//! mode with Kiln's own profile words (`Ast::expr_profile_words`) for
//! every expression of every pair test's sides:
//!
//! 1. every spelling of a profile gives the profile's bytes: two
//!    expressions Kiln profiles alike give the same bytes (a failure is a
//!    row to leave out);
//! 2. two different profiles of one parse never give the same bytes (a
//!    failure is a row to add).
//!
//! An expression whose walk met a kind with no equivalence rule (walked
//! in full, `P::Full`) or carries a cause is left out of both.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use slip_kiln::kiln::ast::{DeclId, ExprId};
use slip_model::keys::RuleTable;

use super::FULL;
use crate::Input;
use crate::stream::Stream;
use crate::walk::Walk;

/// Every test directory (one holding `expect.txt`, `t.cpp` or `a.cpp`)
/// under `tests/<root>`.
pub(crate) fn test_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    let mut subs: Vec<PathBuf> = rd.filter_map(|d| Some(d.ok()?.path())).collect();
    subs.sort();
    if subs
        .iter()
        .any(|p| p.file_name().is_some_and(|n| n == "expect.txt" || n == "t.cpp" || n == "a.cpp"))
    {
        out.push(root.to_path_buf());
        return;
    }
    for s in subs {
        if s.is_dir() {
            test_dirs(&s, out);
        }
    }
}

/// The value of header line `// <key>:` among the first lines of `main`.
fn header<'t>(main: &'t str, key: &str) -> Option<&'t str> {
    main.lines().take(8).find_map(|l| l.strip_prefix(key)).map(str::trim)
}

/// Runs `f` on each side of the pair test in `dir` that parses
/// (`side.h`, `// std:`, `// args:`, `// target:`).
pub(crate) fn each_input(dir: &Path, mut f: impl FnMut(&Input<'_>)) {
    let mut all: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("the test")
        .filter_map(|d| {
            let p = d.ok()?.path();
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some((name, std::fs::read_to_string(&p).ok()?))
        })
        .collect();
    all.sort();
    let get = |n: &str| all.iter().find(|(m, _)| m == n).map(|(_, t)| t.clone());
    for b in [false, true] {
        let mut list: Vec<(String, String)> = Vec::new();
        let main = if let Some(t) = get("t.cpp") {
            list.push(("side.h".into(), if b { "#define B 1\n".into() } else { String::new() }));
            t
        } else {
            match get(if b { "b.cpp" } else { "a.cpp" }) {
                Some(t) => t,
                None => return,
            }
        };
        list.push(("main.cpp".into(), main.clone()));
        for (n, t) in &all {
            if !matches!(n.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
                list.push((n.clone(), t.clone()));
            }
        }
        let std = header(&main, "// std:").unwrap_or("c++20").to_owned();
        let args: Vec<&str> =
            header(&main, "// args:").map(|a| a.split(' ').collect()).unwrap_or_default();
        let target = header(&main, "// target:");
        let refs: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let snippet = slip_kiln::snippet::Snippet {
            files: &refs,
            main: "main.cpp",
            include: &[""],
            std: &std,
        };
        let extra = slip_kiln::snippet::Extra { args: &args, target };
        let _ = slip_kiln::snippet::parse_with_tables(&snippet, &extra, |p| {
            let path = |_| Vec::new();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            f(&input);
        });
    }
}

/// Runs `f` on a walk over each side of the pair test in `dir`.
pub(crate) fn each_side(dir: &Path, mut f: impl FnMut(&mut Walk<'_>)) {
    each_input(dir, |input| {
        let rules = RuleTable::empty();
        let mut w = Walk::new(input, &rules);
        f(&mut w);
    });
}

/// The causes of the units named `name` (exactly) of `input`'s digest.
fn causes_of(input: &Input<'_>, name: &str) -> Vec<Vec<crate::Cause>> {
    let rules = RuleTable::empty();
    let o = crate::digest(
        input,
        &crate::Options {
            rules: &rules,
            read_only: crate::ReadOnly::NO_FILE,
            detail: crate::Detail::All,
            names: true,
            rule_use: false,
        },
    );
    o.names
        .iter()
        .filter(|(_, n)| n == name)
        .map(|(k, _)| o.causes.iter().filter(|(c, _)| c == k).map(|(_, c)| *c).collect())
        .collect()
}

/// The bytes of profile mode for `e` in a key (no unit ordinal), or why
/// the nets leave it out: `full` when the walk met a kind walked in full,
/// else the stream's cause.
pub(crate) fn key_bytes(w: &mut Walk<'_>, e: ExprId) -> Result<Vec<u8>, String> {
    FULL.with(|c| c.set(0));
    w.in_key += 1;
    let mut s = Stream::new();
    crate::expr_dep::profile_expr(w, &mut s, e);
    w.in_key -= 1;
    if FULL.with(std::cell::Cell::get) > 0 {
        return Err("full".to_owned());
    }
    match s.cause() {
        Some(c) => Err(format!("{c:?}")),
        None => Ok(s.as_bytes().to_vec()),
    }
}

/// A finding of a net: the test, the side, the two expressions' kinds.
fn finding(dir: &Path, side: usize, w: &Walk<'_>, a: ExprId, b: ExprId) -> String {
    let kind = |e: ExprId| w.ast.exprs.get(e).kind();
    format!(
        "{} side {side}: {:?} {:?} / {:?} {:?}",
        dir.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(dir).display(),
        a,
        kind(a),
        b,
        kind(b)
    )
}

/// The bytes of the `Ref` of `d`, as a key writes it.
fn ref_bytes(w: &mut Walk<'_>, d: DeclId) -> Vec<u8> {
    w.in_key += 1;
    let mut s = Stream::new();
    w.write_ref(&mut s, d);
    w.in_key -= 1;
    s.as_bytes().to_vec()
}

/// Whether two profiles of one byte string differ only in `D_DECL` words
/// naming declarations of one entity (one `Ref`), which Kiln's profile tells apart by
/// `DeclId` (`D_DECL`, `first(d).raw()`, `profile_expr.rs:370`) and the
/// digest writes as the entity's one `Ref`: a block-scope `extern` and the
/// declaration an instantiation of its function makes again (WP-032),
/// equal by [basic.link]/8. Not a row to add: the digest is right and the
/// profile is finer than equivalence there.
fn one_entity(w: &mut Walk<'_>, a: &[u32], b: &[u32]) -> bool {
    const D_DECL: u32 = 4;
    if a.len() != b.len() {
        return false;
    }
    let mut any = false;
    for i in 0..a.len() {
        if a[i] == b[i] {
            continue;
        }
        let (Some(da), Some(db)) = (DeclId::try_new(a[i]), DeclId::try_new(b[i])) else {
            return false;
        };
        if i == 0
            || a[i - 1] != D_DECL
            || b[i - 1] != D_DECL
            || ref_bytes(w, da) != ref_bytes(w, db)
        {
            return false;
        }
        any = true;
    }
    any
}

/// What the nets found.
#[derive(Default)]
struct Nets {
    same: Vec<String>,
    collide: Vec<String>,
    compared: usize,
    /// The expressions left out, by reason (`key_bytes`).
    left_out: std::collections::BTreeMap<String, usize>,
    /// The pairs `one_entity` excused.
    one_entity: usize,
}

impl Nets {
    /// Both nets over every expression of one parse.
    fn run(&mut self, w: &mut Walk<'_>, dir: &Path, side: usize) {
        // Kiln's words → (bytes, the first expression), and back.
        let mut by_words: HashMap<Vec<u32>, (Vec<u8>, ExprId)> = HashMap::new();
        let mut by_bytes: HashMap<Vec<u8>, (Vec<u32>, ExprId)> = HashMap::new();
        for i in 1..=w.ast.exprs.len() {
            let Some(e) = u32::try_from(i).ok().and_then(ExprId::try_new) else { continue };
            let words = w.ast.expr_profile_words(e);
            let bytes = match key_bytes(w, e) {
                Ok(b) => b,
                Err(why) => {
                    *self.left_out.entry(why).or_default() += 1;
                    continue;
                }
            };
            self.compared += 1;
            match by_words.get(&words) {
                Some((b, first)) if *b != bytes => self.same.push(finding(dir, side, w, *first, e)),
                Some(_) => {}
                None => {
                    by_words.insert(words.clone(), (bytes.clone(), e));
                }
            }
            match by_bytes.get(&bytes) {
                Some((k, first)) if *k != words => {
                    let (k, first) = (k.clone(), *first);
                    if one_entity(w, &k, &words) {
                        self.one_entity += 1;
                    } else {
                        self.collide.push(finding(dir, side, w, first, e));
                    }
                }
                Some(_) => {}
                None => {
                    by_bytes.insert(bytes, (words, e));
                }
            }
        }
    }

    /// Fails on a finding of either net, on fewer than `min` expressions
    /// compared, and on more expressions left out for a reason than its
    /// ceiling in `ceilings` (a reason not listed has ceiling 0), so a
    /// change that turns rows into causes fails too. A ceiling is a share
    /// of the expressions met (compared and left out), in thousandths: a
    /// count would have to be raised with every test added.
    fn check(&self, min: usize, ceilings: &[(&str, usize)], one_entity: usize) {
        eprintln!(
            "{} expressions compared, left out {:?}, {} excused (one entity); net 1: {} \
             failures, net 2: {} failures",
            self.compared,
            self.left_out,
            self.one_entity,
            self.same.len(),
            self.collide.len()
        );
        assert!(self.compared > min, "the nets compared {} expressions", self.compared);
        assert!(
            self.same.is_empty() && self.collide.is_empty(),
            "net 1 (one profile, two byte strings):\n{}\n\nnet 2 (two profiles, one byte \
             string):\n{}",
            self.same.join("\n"),
            self.collide.join("\n")
        );
        let met = self.compared + self.left_out.values().sum::<usize>();
        for (why, &n) in &self.left_out {
            let share = ceilings.iter().find(|(c, _)| c == why).map_or(0, |&(_, c)| c);
            assert!(
                n * 1000 <= met * share,
                "{n} of {met} expressions left out for {why}, ceiling {share} in a thousand"
            );
        }
        assert!(self.one_entity <= one_entity, "{} pairs excused as one entity", self.one_entity);
    }
}

/// Nets (1) and (2) over every pair test.
#[test]
fn the_bytes_are_a_function_of_kilns_profile() {
    let mut dirs = Vec::new();
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for root in ["differ", "equal", "demand", "indeterminate"] {
        test_dirs(&tests.join(root), &mut dirs);
    }
    let mut nets = Nets::default();
    for dir in &dirs {
        let mut side = 0;
        each_side(dir, |w| {
            side += 1;
            nets.run(w, dir, side);
        });
    }
    eprintln!("{} tests", dirs.len());
    nets.check(13_000, PAIR_CEILINGS, 2);
}

/// Sources of the nets beside the pair tests: several spellings of one
/// profile in one parse (renamed parameters, another found set, another
/// qualifier), and profiles that differ in one fact only (a parameter's
/// type, a requires-expression's parameter, a depth).
const OWN: &[&str] = &[
    "int g(int);\ntemplate<class T> auto f(T t) -> decltype(g(t));\nlong g(long);\n\
     template<class T> auto h(T u) -> decltype(g(u));\n",
    "template<class T, class U> auto f(T a, U b) -> decltype(a + b);\n\
     template<class T, class U> auto g(U a, T b) -> decltype(a + b);\n\
     template<class T, class U> auto h(T x, U y) -> decltype(x + y);\n",
    "template<class T> concept C = true;\n\
     template<class T> requires requires (T x, T y) { x + 1; } void f(T);\n\
     template<class T> requires requires (T x, T y) { y + 1; } void g(T);\n\
     template<class T> requires requires (T a, T b) { a + 1; } void h(T);\n\
     template<class T> requires requires (T* x) { x + 1; } void k(T);\n",
    "namespace N { template<class T> struct S { static int m; }; }\n\
     template<class T> auto f(T) -> decltype(N::S<T>::m);\n\
     using namespace N;\ntemplate<class T> auto g(T) -> decltype(S<T>::m);\n\
     template<class T> auto h(T p) -> decltype(p.template q<1>());\n\
     template<class T> auto k(T p) -> decltype(p->~T());\n",
    "template<class T> auto f(T a, auto (*g)(T b) -> decltype(a + b)) -> int;\n\
     template<class T> auto f2(T a, auto (*g)(T b) -> decltype(b + b)) -> int;\n\
     template<int... N> auto v() -> decltype((N + ... + 0));\n\
     template<int... N> auto w() -> decltype((0 + ... + N));\n\
     template<class... T> auto s() -> decltype(sizeof...(T));\n",
    "template<class T> auto f(T a) -> int requires requires (T b) { b + 1; };\n\
     template<class T> auto g(T a) -> int requires requires (T b) { a + 1; };\n\
     template<class T> auto h(T a) -> int requires requires (T* b) { b + 1; };\n\
     template<class T> auto k(T a) -> int requires requires (T b) { { b } noexcept; };\n\
     template<class T> auto l(T a) -> int requires requires (T b) { { b }; };\n\
     template<class T> auto m(T a) -> int requires requires (T b) { 1; };\n\
     template<class T> auto n(T a) -> int requires requires (T* b) { 1; };\n\
     template<class T> auto o(T a, auto (*g)(T c) -> decltype(c + 1)) -> int\n\
       requires requires (T b) { b + 1; };\n",
    "struct A { int m; };\nstruct B { int m; };\n\
     template<class T> auto f(T a, T b) -> decltype(a.A::m);\n\
     template<class T> auto g(T a, T b) -> decltype(a.B::m);\n\
     template<class T> auto h(T a, T b) -> decltype(a.m);\n\
     template<class T> auto k(T a, T b) -> decltype(b.m);\n\
     template<class T> auto l(T* a, T* b) -> decltype(a->m);\n",
    // Item 5 (WP-036 round 2): one fact apart each.
    // A pack's depth: `sizeof...(T)` of the class against `sizeof...(U)`
    // of the member template, both position 0.
    "template<unsigned long N> struct Z {};\n\
     template<class... T> struct S {\n\
       template<class... U> auto f(U...) -> Z<sizeof...(T)>;\n\
       template<class... U> auto g(U...) -> Z<sizeof...(U)>;\n\
     };\n",
    // A cast's kind, `new`'s initialisation style, `this` written or not.
    "template<class T> auto f(T t) -> decltype(static_cast<int>(t));\n\
     template<class T> auto g(T t) -> decltype((int)t);\n\
     template<class T> auto h(T t) -> decltype(new T(t));\n\
     struct S2 { S2(int); };\nvoid* operator new(decltype(sizeof 0), void*) noexcept;\n\
     template<class T> auto n1(T p) -> decltype(new (p) S2(1));\n\
     template<class T> auto n2(T p) -> decltype(new (p) S2{1});\n\
     struct S3 { S3(); };\n\
     template<class T> auto n3(T p) -> decltype(p, new S3);\n\
     template<class T> auto n4(T p) -> decltype(p, new S3());\n\
     template<class T> auto n5(T p) -> decltype(p, new S3{});\n\
     struct I { int a; };\nstruct W { int a[2]; };\nusing IA = I[2];\n\
     template<class T> auto d1(T t) -> decltype(t, W{.a[0] = 1});\n\
     template<class T> auto d2(T t) -> decltype(t, IA{[0].a = 1});\n\
     template<class T> auto k(T t) -> decltype(new T{t});\n\
     template<class T> auto l(T t) -> decltype(new T);\n\
     template<class T> struct S { T m;\n\
       auto f() -> decltype(m);\n  auto g() -> decltype(this->m);\n};\n",
    // Explicit arguments of a variable template and a function template;
    // a name found through a using-declaration against the qualified one.
    "template<int> constexpr int v = 0;\ntemplate<int> int fn();\n\
     template<class T> auto f(T t) -> decltype(t + v<1>);\n\
     template<class T> auto g(T t) -> decltype(t + v<2>);\n\
     template<class T> auto h(T t) -> decltype(t + fn<1>());\n\
     template<class T> auto k(T t) -> decltype(t + fn<2>());\n\
     namespace N { int w; }\nusing N::w;\n\
     template<class T> auto l(T t) -> decltype(t + w);\n\
     template<class T> auto m(T t) -> decltype(t + N::w);\n",
    // Two non-type parameters of different types at one position.
    "template<int N, class T> auto f(T t) -> decltype(t + N);\n\
     template<long N, class T> auto g(T t) -> decltype(t + N);\n",
    // One spelled qualifier naming two classes (the canonical qualifier is
    // not in the profile behind an object of dependent type).
    "struct A { int m; };\nstruct B { int m; };\n\
     namespace P { using X = A; template<class T> auto f(T a) -> decltype(a.X::m); }\n\
     namespace Q { using X = B; template<class T> auto f(T a) -> decltype(a.X::m); }\n",
];

/// The share of the pair tests' expressions each reason leaves out, at
/// most, in thousandths (measured at the merge of WP-036 with the
/// statement tests of WP-034: 31, 192 and 1 of 21 672 expressions; raise
/// one only with the reason the new ones are left out). `full`: a kind
/// walked in full (§4.4); `key/local-ref`: a function body's reference to
/// a local entity, which a key cannot name; `UndeclaredLocal` (was
/// `Pending(Local)` before WP-040): a lambda's closure that no numbering
/// of its owner finds, a lambda in a signature (16 of 22 565 at the
/// merge of WP-040); `local/closure-owner`: a lambda whose closure a
/// namespace or a class owns, which has no key (4); `local/lambda-in-type`:
/// an expression whose type is a closure, profiled in a key as these nets
/// profile every expression (8 of 24 202 at the WP-040 fix round, the
/// call of a `consteval` block's closure among them, which no key holds);
/// `typedef/alignment-by-declaration`: an expression whose type meets a
/// typedef whose declarations disagree on its alignment (digest-kinds
/// §4.2 rule 9; 12 of 33 491 at WP-107's re-check, its own
/// indeterminate tests).
/// The own sources leave nothing out (`OWN_CEILINGS`), which is
/// the exact guard; this one bounds the drift of the pair tests.
const PAIR_CEILINGS: &[(&str, usize)] = &[
    ("full", 50),
    ("Unaccounted(\"key/local-ref\")", 250),
    ("UndeclaredLocal", 5),
    ("Unsupported(\"local/closure-owner\")", 5),
    ("Unsupported(\"local/lambda-in-type\")", 1),
    // WP-041's fix round: `indeterminate/_/spelling-conversion-name-*`
    // and `spelling-fold-operator-*` (a conversion name of a marked type;
    // a fold over an operator some declaration names).
    ("Unaccounted(\"spelling/conversion-name\")", 1),
    ("Unaccounted(\"spelling/fold-operator\")", 1),
    // WP-041's fix round 3: `indeterminate/_/spelling-prefix-splice` (a
    // splice qualifier, 2 of 33 314).
    ("Unsupported(\"Nns/Splice\")", 1),
    ("Unsupported(\"typedef/alignment-by-declaration\")", 1),
];

/// The same for `OWN`.
const OWN_CEILINGS: &[(&str, usize)] = &[];

#[test]
fn the_nets_hold_on_their_own_sources() {
    let mut nets = Nets::default();
    for (i, src) in OWN.iter().enumerate() {
        let label = PathBuf::from(format!("own[{i}]"));
        crate::decl_func::tests::with_walk(src, |w| nets.run(w, &label, 1));
    }
    nets.check(50, OWN_CEILINGS, 0);
}

/// Digests `src` with names; the keys and causes of the units whose name
/// starts with `name`.
fn digest_named(src: &str, name: &str) -> Vec<(slip_model::Digest, Vec<crate::Cause>)> {
    digest_filtered(src, |n| names(n, name))
}

/// Whether display name `n` is of an entity named `name` (`f(`): it
/// begins with it, or, for a function template, whose name begins with
/// its head and return type, holds it after a space.
fn names(n: &str, name: &str) -> bool {
    n.starts_with(name) || (n.starts_with("template <") && n.contains(&format!(" {name}")))
}

/// Digests `src` with names; the keys and causes of the units whose name
/// passes `keep`.
fn digest_filtered(
    src: &str,
    keep: impl Fn(&str) -> bool,
) -> Vec<(slip_model::Digest, Vec<crate::Cause>)> {
    digest_all(src).into_iter().filter(|(_, n, _)| keep(n)).map(|(k, _, c)| (k, c)).collect()
}

/// Digests `src` with names: each unit's key, name and causes.
fn digest_all(src: &str) -> Vec<(slip_model::Digest, String, Vec<crate::Cause>)> {
    let files = [("main.cpp", src)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    slip_kiln::snippet::parse_with_tables(&snippet, &slip_kiln::snippet::Extra::default(), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let rules = RuleTable::empty();
        let o = crate::digest(
            &input,
            &crate::Options {
                rules: &rules,
                read_only: crate::ReadOnly::NO_FILE,
                detail: crate::Detail::All,
                names: true,
                rule_use: false,
            },
        );
        o.names
            .iter()
            .map(|(k, n)| {
                let causes = o.causes.iter().filter(|(c, _)| c == k).map(|(_, c)| *c).collect();
                (*k, n.clone(), causes)
            })
            .collect()
    })
    .expect("the snippet parses")
}

/// F1: `h`'s found set is left out of its profile, so its key is the
/// same whichever `g` lookup found; its unit, which writes the type
/// through `absorb_type`, adds its own spelling's set to its list
/// (WP-041), and a key does not.
#[test]
#[allow(clippy::many_single_char_names, reason = "sides a and b, as the review names them")]
fn a_left_out_found_set_marks_the_unit() {
    let a = "int g(int);\ntemplate<class T> auto f(T t) -> decltype(g(t));\nlong g(long);\n\
             template<class T> auto h(T t) -> decltype(g(t));\n";
    let b = "int g(int);\ntemplate<class T> auto f(T t) -> decltype(g(t));\n\
             template<class T> auto h(T t) -> decltype(g(t));\nlong g(long);\n";
    let ha = digest_named(a, "h(");
    let hb = digest_named(b, "h(");
    assert_eq!(ha.len(), 1);
    assert_eq!(hb.len(), 1);
    assert_eq!(ha[0].0, hb[0].0, "the key ignores the mark");
    for (src, marked, sets) in [
        (a, true, 2),
        (b, true, 1),
        // An empty operator set raises the mark too: the mark is a
        // function of the profile.
        ("template<class T> auto h(T t) -> decltype(t + 1);\n", true, 0),
        ("template<class T> auto h(T t) -> decltype(t.m);\n", false, 0),
    ] {
        crate::decl_func::tests::with_walk(src, |w| {
            let h = crate::decl_func::tests::named(
                w,
                "h",
                slip_kiln::kiln::ast::decl::DeclKind::Function,
            );
            let q = w.ast.decls.get(h[0]).ty().expect("a type");
            let t = w.type_digest(q, false);
            assert_eq!(t.spelling, marked, "{src}");
            let mut s = Stream::new();
            w.absorb_type(&mut s, t, q, None);
            assert_eq!(s.cause(), None, "{src}");
            assert_eq!(w.unit.facts_due, sets > 0, "{src}");
            let found: Vec<usize> = w
                .unit
                .facts
                .iter()
                .filter_map(|f| match f {
                    crate::own_spelling::Fact::Found(ds) => Some(ds.len()),
                    _ => None,
                })
                .collect();
            assert_eq!(found, if sets > 0 { vec![sets] } else { vec![] }, "{src}");
            w.unit = crate::walk::UnitState::default();
            w.in_key += 1;
            let mut s = Stream::new();
            w.absorb_type(&mut s, t, q, None);
            w.in_key -= 1;
            assert_eq!(s.cause(), None, "a key ignores the mark");
            assert!(w.unit.facts.is_empty(), "a key reads no spelling");
            // A memo hit marks the stream around it too.
            let before = w.left_out;
            let again = w.type_digest(q, false);
            assert_eq!(again, t);
            assert_eq!(w.left_out > before, marked);
        });
    }
}

/// `n` parameters, each typed by `decltype` of the one before used twice.
fn chain(n: usize) -> String {
    let mut s = String::from("template<class T> auto f(T p0");
    for i in 1..n {
        let _ = write!(s, ", decltype(p{0}+p{0}) p{i}", i - 1);
    }
    let _ = writeln!(s, ") -> decltype(p{0}+p{0});", n - 1);
    s
}

/// F4: a type met again below a depth-limit hit is not walked again
/// within one outermost type digest: each chain is digested in a bounded
/// time, and the ones deeper than `DEPTH_LIMIT` are too deep.
#[test]
fn a_chain_at_the_depth_limit_is_linear() {
    for (n, deep) in [(190, false), (205, true), (400, true)] {
        let src = chain(n);
        // On a thread of its own, so that a walk that does not end fails
        // the test instead of hanging it.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .stack_size(crate::STACK_SIZE)
            .spawn(move || {
                let _ = tx.send(digest_named(&src, "f("));
            })
            .expect("a thread");
        let f = rx
            .recv_timeout(std::time::Duration::from_mins(1))
            .unwrap_or_else(|_| panic!("n = {n}: not done in 60 s"));
        assert_eq!(f.len(), 1, "n = {n}");
        assert_eq!(f[0].1.contains(&crate::walk::TOO_DEEP), deep, "n = {n}: {:?}", f[0].1);
    }
}

/// F7: a local entity used inside a nested profile (the type of a
/// parameter the outer profile names) is a local use of the unit: the
/// outer type is kept per unit, not per parse.
#[test]
fn a_local_use_carries_out_of_nested_profiles() {
    let src = "void u() {\n  static int x;\n  \
               auto l = [](auto a, decltype(a + x) b) -> decltype(b) { return b; };\n}\n";
    crate::decl_func::tests::with_walk(src, |w| {
        let refs: Vec<ExprId> = (1..=w.ast.exprs.len())
            .filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?))
            .filter(|&e| {
                let n = w.ast.exprs.get(e);
                n.kind() == slip_kiln::kiln::ast::expr::ExprKind::DeclRefExpr
                    && w.ast.decls.get(w.ast.exprs.decl(e)).kind()
                        == slip_kiln::kiln::ast::decl::DeclKind::ParmVar
                    && w.ast.decls.param(w.ast.exprs.decl(e)).is_some_and(|p| p.index == 1)
            })
            .collect();
        let b = *refs.first().expect("a reference to b");
        let uses = w.unit.local_uses;
        let mut s = Stream::new();
        crate::expr_dep::profile_expr(w, &mut s, b);
        assert!(w.unit.local_uses > uses, "the use of x reached the unit");
        assert!(!w.unit.types.is_empty(), "b's type is kept per unit");
        assert_eq!(s.cause(), None);
    });
}

/// The two sides of a pair whose found set differs: `g(T())` finds
/// `g(long)` alone on side A and both `g` on side B (where `char g(int)`
/// is declared before the use); for `T = int` side A means 4 and side B 1.
fn found_set_sides(use_: &str) -> [String; 2] {
    let head = "int g(long);\ntemplate<unsigned long N> constexpr int h() { return N; }\n\
                template<unsigned long N> struct Z {};\n\
                template<class T, unsigned long N> struct Y;\n\
                template<class T, class U> struct W;\n";
    [format!("{head}{use_}char g(int);\n"), format!("{head}char g(int);\n{use_}")]
}

/// The causes of the unit named `name`, its key, and every unit's name
/// (for the message).
fn unit_causes(src: &str, name: &str) -> (slip_model::Digest, Vec<crate::Cause>, String) {
    let all = digest_all(src);
    let list = all.iter().map(|(_, n, c)| format!("{n} {c:?}")).collect::<Vec<_>>().join("; ");
    let (k, _, c) = all
        .into_iter()
        .find(|(_, n, _)| n == name)
        .unwrap_or_else(|| panic!("no unit {name}: {list}"));
    (k, c, list)
}

/// Item 1 (F-A, F-D), WP-041: a profile written straight into a unit
/// stream (a template argument of a call in a member's initialiser, a
/// partial specialisation's argument) or through a type (an array bound,
/// an argument type) whose found set was left out: the key is the same on
/// both sides, the unit determinate on both and different (its list holds
/// its own spelling's set).
#[test]
fn a_left_out_found_set_in_a_unit_is_written() {
    let cases = [
        ("template<class T> struct S { static constexpr int v = h<sizeof(g(T()))>(); };\n", "S::v"),
        (
            "template<class T> struct Y<T, sizeof(g(T()))> {};\n",
            "Y<type-parameter-0-0, sizeof (g(T()))>",
        ),
        (
            "template<class T> struct W<T, Z<sizeof(g(T()))>> {};\n",
            "W<type-parameter-0-0, Z<sizeof (g(T()))>>",
        ),
        ("template<class T> struct A { char a[sizeof(g(T()))]; };\n", "A::a"),
    ];
    for (use_, name) in cases {
        let [a, b] = found_set_sides(use_);
        let (ka, ca, la) = unit_causes(&a, name);
        let (kb, cb, _) = unit_causes(&b, name);
        assert_eq!(ka, kb, "{name}: the key ignores the found set");
        // `S::v` is `Pending(Demand)` (its initialiser's call), a cause of
        // its own; none is a spelling's.
        let spelling = |c: &crate::Cause| matches!(c, crate::Cause::Unaccounted(w) if w.starts_with("spelling/"));
        assert!(!ca.iter().any(spelling), "{name} side A: {ca:?} ({la})");
        assert!(!cb.iter().any(spelling), "{name} side B: {cb:?}");
        assert_ne!(unit_digest(&a, name), unit_digest(&b, name), "{name}: the sets differ");
    }
}

/// The unit digest of the unit named `name`.
fn unit_digest(src: &str, name: &str) -> slip_model::Digest {
    let files = [("main.cpp", src)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    slip_kiln::snippet::parse_with_tables(&snippet, &slip_kiln::snippet::Extra::default(), |p| {
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let rules = RuleTable::empty();
        let o = crate::digest(
            &input,
            &crate::Options {
                rules: &rules,
                read_only: crate::ReadOnly::NO_FILE,
                detail: crate::Detail::All,
                names: true,
                rule_use: false,
            },
        );
        let key = o.names.iter().find(|(_, n)| n == name).map(|(k, _)| *k).expect("the unit");
        o.digests
            .groups
            .iter()
            .flat_map(|g| g.units.iter().flatten())
            .find(|u| u.key == key)
            .map(|u| u.digest)
            .expect("its digest")
    })
    .expect("the snippet parses")
}

/// Item 7d, WP-041: F1's two pair tests are determinate on both sides
/// (the unit `h`); their expectations are the pair tests' own.
#[test]
fn the_f1_pairs_are_determinate() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differ/_");
    for dir in ["dependent-decltype-own-lookup-set", "dependent-decltype-earlier-spelling-added"] {
        let mut sides = 0;
        each_input(&tests.join(dir), |input| {
            sides += 1;
            let h = causes_of(input, "h(type-parameter-0-0)");
            assert_eq!(h.len(), 1, "{dir}");
            assert!(h[0].is_empty(), "{dir} side {sides}: {:?}", h[0]);
        });
        assert_eq!(sides, 2, "{dir}");
    }
}
/// Item 4 (F-E): two declarations whose signatures hold a lambda are two
/// templates in Kiln (a kind it profiles as unique, `profile_expr.rs:860`)
/// and one key here, whose bytes are the lambda's spelling (walked in
/// full): the key is a `collision`, never one entity in silence.
/// Since WP-040 the closure the signature names (its type) has no number:
/// its owner's numbering reads no key or type (digest §4.3), so the key is
/// indeterminate (`UndeclaredLocal`) before a collision can show; either
/// way never one entity in silence.
#[test]
fn lambdas_in_signatures_collide() {
    for (src, name) in [
        (
            "template<int N> void foo(const char(*)[([]{}, N)]);\n\
             template<int N> void foo(const char(*)[([]{}, N)]);\n",
            "foo(",
        ),
        (
            "template<class T> void spam(decltype([]{})(*)[sizeof(T)]);\n\
             template<class T> void spam(decltype([]{})(*)[sizeof(T)]);\n",
            "spam(",
        ),
    ] {
        let all = digest_all(src);
        // The operand unit (WP-156 fix round 2: the dependent `decltype`
        // is walked for holders, and a lambda in it fails closed) is
        // indeterminate beside its parent.
        let (operands, units): (Vec<_>, Vec<_>) = all
            .iter()
            .filter(|(_, n, _)| names(n, name))
            .partition(|(_, n, _)| n.ends_with("::<operands>"));
        assert_eq!(units.len(), 1, "{all:?}");
        assert_eq!(units[0].2, [crate::Cause::UndeclaredLocal], "{name}");
        assert!(operands.iter().all(|(_, _, c)| !c.is_empty()), "{all:?}");
    }
}

/// Item 6 (F-G): a key computed inside a type digest does not raise the
/// type's mark, so whether a type is marked does not depend on whether
/// the key was computed first. `Y*` names the partial specialisation,
/// whose key holds a profile with a left-out found set.
#[test]
fn the_mark_does_not_depend_on_which_key_came_first() {
    use slip_kiln::kiln::ast::decl::DeclKind;
    let src = "int g(long);\ntemplate<class T, unsigned long N> struct Y;\n\
               template<class T> struct Y<T, sizeof(g(T()))> { Y* p; };\nchar g(int);\n";
    let mut got = Vec::new();
    for key_first in [false, true] {
        crate::decl_func::tests::with_walk(src, |w| {
            let of = crate::decl_func::tests::of_kind;
            let spec = w
                .ast
                .decls
                .ids()
                .find(|&d| w.ast.partial_spec_info(d).is_some())
                .expect("Y<T, ..>");
            let field = of(w, DeclKind::Field)[0];
            if key_first {
                let _ = w.key(spec);
            }
            let q = w.ast.decls.get(field).ty().expect("a type");
            got.push(w.type_digest(q, false));
        });
    }
    assert_eq!(got[0], got[1]);
    assert!(!got[0].spelling, "the key's left-out fact is the specialisation's unit's");
}

/// What every pinning sample may use.
const PRELUDE: &str = "template<unsigned long N> struct Z {};\n\
    struct S { int m; S(); S(int); int f(); int f(int); static int g(); int operator+(int); \
    bool operator==(const S&) const; };\n\
    struct D { ~D(); int m; };\n\
    struct R { const int &r; };\n\
    int fn(int);\nint fn(long);\nint fd(int = 1);\n\
    template<class T> constexpr int vt = 0;\ntemplate<int> int ft();\n\
    template<class T> concept C = true;\n\
    void *operator new(decltype(sizeof 0), void *) noexcept;\n\
    namespace N { template<class T> struct A { static int m; }; }\n\
    namespace NA = N;\n\
    namespace std { class type_info; }\n";

/// The samples that pin profile mode's bytes (item 3, WP-036 round 2):
/// each is a source holding a function `f` whose return type holds one
/// profile (a `decltype` operand, or the first profile argument of a
/// specialisation), named by the rows of `spec/digest-kinds.md` §4.4
/// whose sample column names it.
const SAMPLES: &[(&str, &str)] = &[
    ("decl-template-parm", "template<class... T> auto f(T... t) -> decltype(sizeof...(T));"),
    ("decl-nttp", "template<int N, class T> auto f(T t) -> decltype(t + N);"),
    ("decl-parm", "template<class T> auto f(T t) -> decltype(t);"),
    ("decl-parm-requires", "template<class T> auto f(T t) -> Z<requires (T u) { u + 1; }>;"),
    ("decl-other", "template<class T> auto f(T t) -> decltype(t + S::g());"),
    ("nttp-type-placeholder", "template<auto N, class T> auto f(T t) -> decltype(t + N);"),
    ("args-list", "template<class T> auto f(T t) -> decltype(t + vt<T>);"),
    ("args-each", "template<class T> auto f(T t) -> decltype(t.template h<1>());"),
    ("ref-args-implied", "template<class T> auto f(T t) -> decltype(t + N::A<int>::m);"),
    ("spelled-nns", "template<class T> auto f(T t) -> decltype(t.S::m);"),
    ("spelled-nns-namespace", "template<class T> auto f(T t) -> decltype(t.NA::A<int>::m);"),
    ("ctad", "template<class T> auto f(T t) -> decltype(N::A(t));"),
    ("ImplicitCastExpr", "template<class T> auto f(T t) -> decltype(t + fn(1L));"),
    ("MaterializeTemporaryExpr", "template<class T> auto f(T t) -> decltype(t + R{1}.r);"),
    ("CXXBindTemporaryExpr", "template<class T> auto f(T t) -> decltype(t + D().m);"),
    (
        "SubstNonTypeTemplateParmExpr",
        "template<int N> struct B { template<class U> static auto f(U u) -> decltype(u + N); };\n\
         auto p = &B<3>::f<int>;",
    ),
    ("IntegerLiteral", "template<class T> auto f(T t) -> decltype(t + 1);"),
    ("FloatingLiteral", "template<class T> auto f(T t) -> decltype(t + 1.5);"),
    ("CharacterLiteral", "template<class T> auto f(T t) -> decltype(t + u'x');"),
    ("StringLiteral", "template<class T> auto f(T t) -> decltype(t + \"ab\");"),
    ("CXXBoolLiteralExpr", "template<class T> auto f(T t) -> decltype(t + true);"),
    ("CXXNullPtrLiteralExpr", "template<class T> auto f(T t) -> decltype(t + nullptr);"),
    ("GNUNullExpr", "template<class T> auto f(T t) -> decltype(t + __null);"),
    ("CXXScalarValueInitExpr", "template<class T> auto f(T t) -> decltype(t + int());"),
    ("MemberExpr", "template<class T> auto f(T t) -> decltype(t + S().m);"),
    ("CXXThisExpr", "template<class T> struct B { T m; auto f() -> decltype(this->m); };"),
    ("SourceLocExpr", "template<class T> auto f(T t) -> decltype(t + __builtin_LINE());"),
    ("UnresolvedLookupExpr", "template<class T> auto f(T t) -> decltype(fn(t));"),
    (
        "UnresolvedLookupExpr-ttp",
        "template<template<class> auto V, class T> auto f(T t) -> decltype(t + V<T>);",
    ),
    ("DependentScopeDeclRefExpr", "template<class T> auto f(T t) -> decltype(T::value + t);"),
    ("CXXDependentScopeMemberExpr", "template<class T> auto f(T *t) -> decltype(t->m);"),
    ("SizeOfPackExpr", "template<class... T> auto f(T... t) -> decltype(sizeof...(t));"),
    ("UnaryOperator", "template<class T> auto f(T t) -> decltype(-t);"),
    ("ParenExpr", "template<class T> auto f(T t) -> decltype((t));"),
    ("PackExpansionExpr", "template<class... T> auto f(T... t) -> decltype(fn(t...));"),
    ("CXXNoexceptExpr", "template<class T> auto f(T t) -> Z<noexcept(t + 1)>;"),
    ("BinaryOperator", "template<class T> auto f(T t) -> decltype(t * 2);"),
    ("ArraySubscriptExpr", "template<class T> auto f(T t) -> decltype(t[0]);"),
    ("PackIndexingExpr", "template<class... T> auto f(T... t) -> decltype(t...[0]);"),
    ("CompoundAssignOperator", "template<class T> auto f(T t) -> decltype(t += 1);"),
    ("ConditionalOperator", "template<class T> auto f(T t) -> decltype(t ? 1 : 2);"),
    ("CXXFoldExpr", "template<class... T> auto f(T... t) -> decltype((t + ...));"),
    ("CallExpr", "template<class T> auto f(T t) -> decltype(t(1));"),
    ("CXXMemberCallExpr", "template<class T> auto f(T t) -> decltype(t + S().f(1));"),
    ("CXXOperatorCallExpr", "template<class T> auto f(T t) -> decltype(t + (S() + 1));"),
    ("CXXRewrittenBinaryOperator", "template<class T> auto f(T t) -> decltype(t + (S() != S()));"),
    ("BinaryConditionalOperator", "template<class T> auto f(T t) -> decltype(t ?: 1);"),
    ("ParenListExpr", "template<class T> auto f(T t) -> decltype(new (t) S());"),
    ("InitListExpr", "template<class T> auto f(T t) -> decltype(T{t, 1});"),
    (
        "DesignatedInitExpr",
        "struct E { int a[2]; int b; };\n\
         template<class T> auto f(T t) -> decltype(E{.a[1] = t, .b = 0});",
    ),
    ("CStyleCastExpr", "template<class T> auto f(T t) -> decltype((int)t);"),
    ("CXXFunctionalCastExpr", "template<class T> auto f(T t) -> decltype(t + int(1L));"),
    ("CXXStaticCastExpr", "template<class T> auto f(T t) -> decltype(static_cast<int>(t));"),
    ("CXXConstCastExpr", "template<class T> auto f(T *t) -> decltype(const_cast<const T *>(t));"),
    (
        "CXXReinterpretCastExpr",
        "template<class T> auto f(T t) -> decltype(reinterpret_cast<long>(t));",
    ),
    ("CXXDynamicCastExpr", "template<class T> auto f(T *t) -> decltype(dynamic_cast<T *>(t));"),
    (
        "BuiltinBitCastExpr",
        "template<class T> auto f(T t) -> decltype(__builtin_bit_cast(int, t));",
    ),
    ("CXXUnresolvedConstructExpr", "template<class T> auto f(T t) -> decltype(T(t));"),
    ("CXXParenListInitExpr", "template<class T> auto f(T t) -> decltype(t + R(1).r);"),
    ("CXXConstructExpr", "template<class T> auto f(T t) -> decltype(t + S(1).m);"),
    ("CXXTemporaryObjectExpr", "template<class T> auto f(T t) -> decltype(t + S{}.m);"),
    ("CompoundLiteralExpr", "template<class T> auto f(T t) -> decltype(t + (int){1});"),
    ("CXXDefaultArgExpr", "template<class T> auto f(T t) -> decltype(t + fd());"),
    ("CXXNewExpr", "template<class T> auto f(T *p) -> decltype(new (p) S(1));"),
    ("CXXNewExpr-braces", "template<class T> auto f(T *p) -> decltype(new (p) S{1});"),
    ("CXXDeleteExpr", "template<class T> auto f(T *p) -> decltype(delete p);"),
    ("CXXThrowExpr", "template<class T> auto f(T t) -> decltype(throw t);"),
    ("CXXPseudoDestructorExpr", "template<class T> auto f(T t) -> decltype(t.~T());"),
    ("CXXTypeidExpr", "template<class T> auto f(T t) -> decltype(typeid(t));"),
    ("CXXTypeidExpr-type", "template<class T> auto f(T t) -> decltype(t, typeid(T));"),
    ("UnaryExprOrTypeTraitExpr", "template<class T> auto f(T t) -> Z<sizeof(t)>;"),
    ("UnaryExprOrTypeTraitExpr-type", "template<class T> auto f(T t) -> Z<alignof(T)>;"),
    ("TypeTraitExpr", "template<class T> auto f(T t) -> Z<__is_same(T, int)>;"),
    ("ArrayTypeTraitExpr", "template<class T> auto f(T t) -> Z<__array_rank(T)>;"),
    ("ExpressionTraitExpr", "template<class T> auto f(T t) -> Z<__is_lvalue_expr(t)>;"),
    (
        "IntegerPackExpr",
        "template<unsigned long N, class T> auto f(T t) -> decltype((t + ... + __integer_pack(N)));",
    ),
    ("RequiresExpr", "template<class T> auto f(T t) -> Z<requires { t + 1; typename T::x; }>;"),
    (
        "RequiresExpr-compound",
        "template<class T> auto f(T t) -> Z<requires { { t } noexcept -> C; requires true; }>;",
    ),
    ("ConceptSpecializationExpr", "template<class T> auto f(T t) -> Z<C<T>>;"),
    (
        "ConceptSpecializationExpr-ttp",
        "template<template<class> concept K, class T> auto f(T t) -> Z<K<T>>;",
    ),
    ("ImaginaryLiteral", "template<class T> auto f(T t) -> decltype(t + 1i);"),
    ("OffsetOfExpr", "template<class T> auto f(T t) -> decltype(t + __builtin_offsetof(S, m));"),
    ("LambdaExpr", "template<class T> auto f(T t) -> decltype(t + [] { return 1; }());"),
    (
        "ChooseExpr",
        "template<class T> auto f(T t) -> decltype(t + __builtin_choose_expr(1, 1, 2));",
    ),
    ("GenericSelectionExpr", "template<class T> auto f(T t) -> decltype(t + _Generic(1, int: 2));"),
];

/// The key-mode bytes of every profile in the return type of a function
/// named `f` of `src` (the prelude first), in arena order, with the kinds
/// profile mode visited.
fn sample_bytes(src: &str) -> Result<(Vec<String>, HashSet<String>), String> {
    use slip_kiln::kiln::ast::decl::DeclKind;
    let src = format!("{PRELUDE}{src}\n");
    let files = [("main.cpp", src.as_str())];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++26",
    };
    let rules = RuleTable::empty();
    slip_kiln::snippet::parse_with_tables(&snippet, &slip_kiln::snippet::Extra::default(), |p| {
        if p.errors != 0 {
            return Err(p.diagnostics.clone());
        }
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let w = &mut Walk::new(&input, &rules);
        let named = crate::decl_func::tests::named;
        let mut fs = named(w, "f", DeclKind::Function);
        fs.extend(named(w, "f", DeclKind::CXXMethod));
        fs.sort();
        let mut out = Vec::new();
        crate::expr_profile::visited_take();
        for func in fs {
            if let Some(line) = return_profile_bytes(w, func) {
                out.push(line);
            }
        }
        let seen =
            crate::expr_profile::visited_take().into_iter().map(|k| format!("{k:?}")).collect();
        Ok((out, seen))
    })
    .unwrap_or_else(|e| Err(format!("{e:?}")))
}

/// The key-mode bytes, in hex, of the profile of `func`'s return type (a
/// `decltype` operand, or the first profile argument of a specialisation),
/// and the cause if any.
fn return_profile_bytes(walk: &mut Walk<'_>, func: DeclId) -> Option<String> {
    use slip_kiln::kiln::ast::types::{ArgView, TypeView};
    let types = &walk.ast.types;
    let TypeView::Function(proto) =
        types.view(types.canonical(walk.ast.decls.get(func).ty()?).ty())
    else {
        return None;
    };
    let profile = match types.view(types.canonical(proto.ret()).ty()) {
        TypeView::DependentDecltype(p) => Some(p),
        TypeView::TemplateSpecialization { args, .. } => {
            types.args(args).iter().find_map(|arg| match arg.view() {
                ArgView::Profile(p) => Some(p),
                _ => None,
            })
        }
        _ => None,
    };
    let root = types.profile_expr(profile?)?;
    walk.in_key += 1;
    let mut stream = Stream::new();
    crate::expr_dep::profile_expr(walk, &mut stream, root);
    walk.in_key -= 1;
    let mut hex = String::new();
    for byte in stream.as_bytes() {
        let _ = write!(hex, "{byte:02x}");
    }
    Some(match stream.cause() {
        Some(c) => format!("{hex} {c:?}"),
        None => hex,
    })
}

/// The rows of the two tables of `spec/digest-kinds.md` §4.4: (table,
/// first cell, sample cell).
fn spec_rows() -> Vec<(String, String, String)> {
    let spec = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/digest-kinds.md"),
    )
    .expect("the spec");
    let start = spec.find("### 4.4 ").expect("§4.4");
    let end = start + spec[start..].find("### 4.5 ").expect("§4.5");
    let mut table = String::new();
    let mut rows = Vec::new();
    for line in spec[start..end].lines() {
        if let Some(h) = line.strip_prefix("#### ") {
            table = h.trim().to_owned();
        } else if line.starts_with("### ") {
            table.clear();
        } else if line.starts_with("| `") && !table.is_empty() {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let first = cells[0].trim_matches('`').to_owned();
            rows.push((table.clone(), first, cells[cells.len() - 1].to_owned()));
        }
    }
    rows
}

/// Item 3 (F-C, WP-036 round 2): the bytes of profile mode, pinned by one
/// sample per row of §4.4's tables; `SLIP_BLESS=1` rewrites
/// `tests/golden/profile_bytes.txt`. A row without a sample (or a reason
/// none is possible), a sample no row names, a kind row whose samples do
/// not visit the kind, an `ExprKind` without a row, and a changed byte all
/// fail.
#[test]
fn the_bytes_are_pinned() {
    let rows = spec_rows();
    let labels: HashSet<&str> = SAMPLES.iter().map(|(l, _)| *l).collect();
    let mut named = HashSet::new();
    let mut bad = Vec::new();
    let mut seen_by = HashMap::new();
    let mut golden = String::new();
    for (label, src) in SAMPLES {
        let (bytes, seen) = match sample_bytes(src) {
            Ok(r) => r,
            Err(e) => {
                bad.push(format!("sample {label}: {}", e.lines().next().unwrap_or("")));
                continue;
            }
        };
        if bytes.is_empty() {
            bad.push(format!("sample {label}: no profile in f's return type"));
        }
        for b in &bytes {
            let _ = writeln!(golden, "{label}: {b}");
        }
        seen_by.insert(*label, seen);
    }
    for (table, first, cell) in &rows {
        if let Some(why) = cell.strip_prefix("none:") {
            if why.trim().is_empty() {
                bad.push(format!("{table} {first}: none without a reason"));
            }
            continue;
        }
        let cited: Vec<&str> = cell.split(',').map(|c| c.trim().trim_matches('`')).collect();
        for c in &cited {
            if !labels.contains(c) {
                bad.push(format!("{table} {first}: no sample {c}"));
            }
            named.insert(*c);
        }
        if table == "Kinds"
            && !cited.iter().any(|c| seen_by.get(c).is_some_and(|s| s.contains(first)))
        {
            bad.push(format!("{table} {first}: no sample of {cited:?} visits it"));
        }
    }
    for l in &labels {
        if !named.contains(l) {
            bad.push(format!("sample {l}: no row names it"));
        }
    }
    let kinds: HashSet<&str> =
        rows.iter().filter(|(t, _, _)| t == "Kinds").map(|(_, f, _)| f.as_str()).collect();
    let tables = include_str!("tables.rs");
    let from = tables.find("fn row(v: &kiln::ast::expr::ExprKind)").expect("ExprKind rows");
    let body = &tables[from..from + tables[from..].find("\n    }\n").expect("end")];
    for k in body.split("kiln::ast::expr::ExprKind::").skip(1) {
        let k = k.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("");
        if !kinds.contains(k) {
            bad.push(format!("ExprKind {k}: no row in §4.4 Kinds"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/profile_bytes.txt");
    let head = "# The key-mode bytes of profile mode per sample (expr_profile_tests.rs\n\
                # `the_bytes_are_pinned`); SLIP_BLESS=1 rewrites it.\n";
    let golden = format!("{head}{golden}");
    if std::env::var_os("SLIP_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &golden).unwrap();
        return;
    }
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    if old != golden {
        let diff: Vec<String> = old
            .lines()
            .zip(golden.lines())
            .filter(|(a, b)| a != b)
            .take(10)
            .map(|(a, b)| format!("- {a}\n+ {b}"))
            .collect();
        panic!(
            "profile bytes changed (SLIP_BLESS=1 to accept): {} lines against {}\n{}",
            golden.lines().count(),
            old.lines().count(),
            diff.join("\n")
        );
    }
}
