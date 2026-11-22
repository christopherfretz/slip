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

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use slip_kiln::kiln::ast::ExprId;
use slip_model::keys::RuleTable;

use super::FULL;
use crate::Input;
use crate::stream::Stream;
use crate::walk::Walk;

/// Every test directory (one holding `expect.txt`) under `tests/<root>`.
fn test_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    let mut subs: Vec<PathBuf> = rd.filter_map(|d| Some(d.ok()?.path())).collect();
    subs.sort();
    if subs.iter().any(|p| p.file_name().is_some_and(|n| n == "expect.txt")) {
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

/// Runs `f` on a walk over each side of the pair test in `dir` that
/// parses (`side.h`, `// std:`, `// args:`, `// target:`).
pub(crate) fn each_side(dir: &Path, mut f: impl FnMut(&mut Walk<'_>)) {
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
        let _ = slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
            let path = |_| Vec::new();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let rules = RuleTable::empty();
            let mut w = Walk::new(&input, &rules);
            f(&mut w);
        });
    }
}

/// The bytes of profile mode for `e` in a key (no unit ordinal), or
/// `None` when the walk met a kind walked in full or carries a cause.
pub(crate) fn key_bytes(w: &mut Walk<'_>, e: ExprId) -> Option<Vec<u8>> {
    FULL.with(|c| c.set(0));
    w.in_key += 1;
    let mut s = Stream::new();
    crate::expr_dep::profile_expr(w, &mut s, e);
    w.in_key -= 1;
    let full = FULL.with(std::cell::Cell::get);
    (full == 0 && s.cause().is_none()).then(|| s.as_bytes().to_vec())
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

/// The tests whose two profiles of one byte string are references to
/// two declarations of one entity, which Kiln's profile tells apart by
/// `DeclId` (`D_DECL`, `first(d).raw()`) and the digest writes as the
/// entity's one `Ref`: a block-scope `extern` and the declaration an
/// instantiation of its function makes again (WP-032), equal by
/// [basic.link]/8. Not a row to add: the digest is right and the
/// profile is finer than equivalence there.
fn one_entity(dir: &Path) -> bool {
    dir.ends_with("demand/_/local-extern-in-instantiated-body")
}

/// What the nets found.
#[derive(Default)]
struct Nets {
    same: Vec<String>,
    collide: Vec<String>,
    compared: usize,
    left_out: usize,
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
            let Some(bytes) = key_bytes(w, e) else {
                self.left_out += 1;
                continue;
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
                Some((k, first)) if *k != words && !one_entity(dir) => {
                    self.collide.push(finding(dir, side, w, *first, e));
                }
                Some(_) => {}
                None => {
                    by_bytes.insert(bytes, (words, e));
                }
            }
        }
    }

    fn check(&self, min: usize) {
        eprintln!(
            "{} expressions compared, {} left out; net 1: {} failures, net 2: {} failures",
            self.compared,
            self.left_out,
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
    nets.check(5_000);
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
];

#[test]
fn the_nets_hold_on_their_own_sources() {
    let mut nets = Nets::default();
    for (i, src) in OWN.iter().enumerate() {
        let label = PathBuf::from(format!("own[{i}]"));
        crate::decl_func::tests::with_walk(src, |w| nets.run(w, &label, 1));
    }
    nets.check(50);
}

/// Digests `src` with names; the keys and causes of the units whose name
/// starts with `name`.
fn digest_named(src: &str, name: &str) -> Vec<(slip_model::Digest, Vec<crate::Cause>)> {
    let files = [("main.cpp", src)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let rules = RuleTable::empty();
        let o = crate::digest(
            &input,
            &crate::Options { rules: &rules, detail: crate::Detail::All, names: true },
        );
        o.names
            .iter()
            .filter(|(_, n)| n.starts_with(name))
            .map(|(k, _)| {
                let causes = o.causes.iter().filter(|(c, _)| c == k).map(|(_, c)| *c).collect();
                (*k, causes)
            })
            .collect()
    })
    .expect("the snippet parses")
}

/// F1: `h`'s found set is left out of its profile, so its key is the
/// same whichever `g` lookup found, and its unit, which writes the type,
/// is pending until spellings are written (WP-041).
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
    // The unit writes h's type through `absorb_type`: marked, it is
    // pending; in a key it is not.
    let spelling = Some(crate::Cause::Pending(crate::Area::Spelling));
    for (src, marked) in
        [(a, true), (b, true), ("template<class T> auto h(T t) -> decltype(t + 1);\n", false)]
    {
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
            w.absorb_type(&mut s, t);
            assert_eq!(s.cause(), if marked { spelling } else { None }, "{src}");
            w.in_key += 1;
            let mut s = Stream::new();
            w.absorb_type(&mut s, t);
            w.in_key -= 1;
            assert_eq!(s.cause(), None, "a key ignores the mark");
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
        let t = std::time::Instant::now();
        let f = digest_named(&src, "f(");
        let took = t.elapsed();
        assert!(took < std::time::Duration::from_secs(30), "n = {n}: {took:?}");
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
