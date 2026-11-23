//! `slip_digest::spelling::check_spellings` (`docs/wp/WP-096.md`): every
//! spelled entity denotes what the transform named, in one observer
//! (`spec/verify.md` §10.2, §10.6; `spec/api.md` §7.2 rule 5).
//!
//! Each case is a post-image whose obligations are marked in the text:
//! `«` and `»` around the bytes `[lo, hi)` of one spelled piece (removed
//! before parsing), and the key each one carries, named by the entity it
//! was meant to denote in the pre-image (`display_name`, the host's name)
//! or given outright. A case with a rename has a pre-image and the rules
//! that undo the rename (`Keys::rename_rule`), as the native rename's run
//! has (`spec/edits.md` §9.3).

use slip_digest::Input;
use slip_digest::query::Keys;
use slip_digest::spelling::{Obligation, Spelling, check_spellings};
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::source::FileId;
use slip_model::Digest;
use slip_model::keys::{Rule, RuleTable};

/// The key an obligation carries.
#[derive(Clone, Copy)]
enum Want<'a> {
    /// The key, with no rules, of the one written entity whose host name
    /// (`display_name`) is this, in the pre-image (the post-image when
    /// the case has none).
    Named(&'a str),
    /// This key.
    Key(Digest),
}

/// One case.
#[derive(Clone, Copy)]
struct Case<'a> {
    /// The post-image, marked; the first file is the main file.
    post: &'a [(&'a str, &'a str)],
    /// The pre-image, unmarked; `None`: the post-image without marks.
    pre: Option<&'a [(&'a str, &'a str)]>,
    /// Renames `(entity, new name)` of the pre-image: the rules that undo
    /// them are the run's rules.
    renames: &'a [(&'a str, &'a str)],
    /// The key of each mark, in the order of the marks (file by file).
    wants: &'a [Want<'a>],
    /// Parse with Kiln's written-reference tables (`ParseOptions::written`).
    tables: bool,
    std: &'a str,
}

const fn case<'a>(post: &'a [(&'a str, &'a str)], wants: &'a [Want<'a>]) -> Case<'a> {
    Case { post, pre: None, renames: &[], wants, tables: true, std: "c++20" }
}

/// A file without its marks, and the marks as `(lo, hi)`.
fn unmark(text: &str) -> (String, Vec<(u32, u32)>) {
    let mut out = String::new();
    let mut marks = Vec::new();
    let mut open: Vec<u32> = Vec::new();
    for c in text.chars() {
        match c {
            '«' => open.push(out.len() as u32),
            '»' => marks.push((open.pop().expect("a » without «"), out.len() as u32)),
            _ => out.push(c),
        }
    }
    assert!(open.is_empty(), "a « without »");
    marks.sort();
    (out, marks)
}

/// Parses `files` (with the tables when `tables`) and gives `f` the unit
/// and the input, on the front end's thread (a `Unit` is not `Sync`; that
/// thread's stack, Kiln's 1 GiB default, is more than
/// `slip_digest::STACK_SIZE`, as a worker's is). The file's subject path
/// is its snippet path.
fn with_unit<R>(
    files: &[(&str, &str)],
    std: &str,
    tables: bool,
    f: impl FnOnce(&slip_kiln::Unit<'_>, &Input<'_>) -> R,
) -> R {
    let owned: Vec<(String, String)> =
        files.iter().map(|(p, t)| ((*p).to_owned(), (*t).to_owned())).collect();
    let refs: Vec<(&str, &str)> = owned.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let snippet =
        slip_kiln::snippet::Snippet { files: &refs, main: refs[0].0, include: &[""], std };
    let extra = slip_kiln::snippet::Extra::default();
    let body = |p: &slip_kiln::snippet::Parsed<'_>| {
        assert_eq!(p.errors, 0, "{}\n{files:?}", p.diagnostics);
        let paths: Vec<Vec<u8>> = p
            .sm
            .file_ids()
            .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
            .collect();
        let path = |f: FileId| paths.get(f.index()).cloned().unwrap_or_default();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        f(p.unit(), &input)
    };
    let r = if tables {
        slip_kiln::snippet::parse_with_tables(&snippet, &extra, body)
    } else {
        slip_kiln::snippet::parse_with(&snippet, &extra, body)
    };
    r.unwrap_or_else(|e| panic!("does not parse: {e}\n{files:?}"))
}

/// Whether `d` was made by instantiation, not written.
fn instantiated(ast: &slip_kiln::kiln::ast::Ast, d: DeclId) -> bool {
    ast.member_spec_info(d).is_some()
        || ast.spec_info(d).is_some_and(|(_, s)| s.kind == SpecKind::ImplicitInstantiation)
}

/// The one written entity whose host name is `name`.
fn entity(keys: &Keys<'_>, input: &Input<'_>, name: &str) -> DeclId {
    let ast = input.ast;
    let mut found: Vec<DeclId> = ast
        .decls
        .ids()
        .filter(|&d| !instantiated(ast, d))
        .map(|d| keys.entity(d))
        .filter(|&e| slip_digest::display_name(ast, e) == name)
        .collect();
    found.sort();
    found.dedup();
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found[0]
}

/// The keys of `names` (no rules) and the rules of `renames` in `files`.
fn pre_keys(
    files: &[(&str, &str)],
    std: &str,
    names: &[&str],
    renames: &[(&str, &str)],
) -> (Vec<Digest>, Vec<Rule>) {
    with_unit(files, std, false, |_, input| {
        let mut keys = Keys::new(input);
        let ks = names
            .iter()
            .map(|n| {
                let e = entity(&keys, input, n);
                keys.key_id(e).unwrap_or_else(|e| panic!("{n}: {e:?}"))
            })
            .collect();
        let rules = renames
            .iter()
            .map(|(n, new)| {
                let e = entity(&keys, input, n);
                keys.rename_rule(e, new.as_bytes()).unwrap_or_else(|e| panic!("{n}: {e:?}"))
            })
            .collect();
        (ks, rules)
    })
}

/// The obligations of `c` and the result, under `extra` obligations
/// after the marked ones.
fn run_with(c: &Case<'_>, extra: &[Obligation]) -> (Vec<Obligation>, Vec<Spelling>) {
    let mut post: Vec<(&str, String)> = Vec::new();
    let mut marks: Vec<(Vec<u8>, u32, u32)> = Vec::new();
    for (p, t) in c.post {
        let (text, ms) = unmark(t);
        marks.extend(ms.into_iter().map(|(lo, hi)| (p.as_bytes().to_vec(), lo, hi)));
        post.push((p, text));
    }
    assert_eq!(marks.len(), c.wants.len(), "one key per mark");
    let post: Vec<(&str, &str)> = post.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let pre = c.pre.unwrap_or(&post);
    let names: Vec<&str> =
        c.wants.iter().filter_map(|w| if let Named(n) = w { Some(*n) } else { None }).collect();
    let (named, rules) = pre_keys(pre, c.std, &names, c.renames);
    let mut named = named.into_iter();
    let mut obligations: Vec<Obligation> = marks
        .into_iter()
        .zip(c.wants)
        .map(|((file, lo, hi), w)| {
            let key = match w {
                Named(_) => named.next().expect("a key per name"),
                Want::Key(k) => *k,
            };
            Obligation { file, lo, hi, key }
        })
        .collect();
    obligations.extend(extra.iter().cloned());
    let table = RuleTable::new(rules).expect("the rules make a table");
    let got = with_unit(&post, c.std, c.tables, |unit, input| {
        check_spellings(unit, input, &table, &obligations)
    });
    assert_eq!(got.len(), obligations.len(), "parallel to the obligations");
    (obligations, got)
}

fn run(c: &Case<'_>) -> Vec<Spelling> {
    run_with(c, &[]).1
}

/// `Fails` with `found` absent or present, for asserting.
fn fails(s: &Spelling) -> Option<(Option<Digest>, String)> {
    match s {
        Spelling::Fails { found, found_name } => Some((*found, found_name.clone())),
        Holds | NotSeen => None,
    }
}

/// The key, with no rules, of the entity named `name` in `files`.
fn key_of(files: &[(&str, &str)], name: &str) -> Digest {
    let post: Vec<(String, String)> =
        files.iter().map(|(p, t)| ((*p).to_owned(), unmark(t).0)).collect();
    let post: Vec<(&str, &str)> = post.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    pre_keys(&post, "c++20", &[name], &[]).0[0]
}

use Spelling::{Holds, NotSeen};
use Want::Named;

// ---------------------------------------------------------------------
// Test group 1: holds (§10.2 rule 5, first point).
// ---------------------------------------------------------------------

// §10.2 rule 3, `spec/api.md` §7.2 rule 1: a sibling spelling behind a
// qualifier that was written.
#[test]
fn s10_2_holds_a_sibling_spelling_behind_a_written_qualifier() {
    let post = [(
        "main.cpp",
        "namespace a { int f(int); int g(int); }\nint use() { return a::«g»(1); }\n",
    )];
    assert_eq!(run(&case(&post, &[Named("a::g(int)")])), [Holds]);
}

// §10.2 rule 3: `::a::b::f` is answered by `f` alone; `a` and `b` end
// before `hi` (`spec/api.md` §7.2 rule 2).
#[test]
fn s10_2_holds_a_fully_qualified_spelling() {
    let post = [(
        "main.cpp",
        "namespace a { namespace b { int f(); } }\nnamespace c { int f(); int use() { return «::a::b::f»(); } }\n",
    )];
    assert_eq!(run(&case(&post, &[Named("a::b::f()")])), [Holds]);
}

// §10.2 rule 4: a call of a function template refers to the
// specialisation; its origin is the template, whose key the obligation
// carries.
#[test]
fn s10_2_holds_a_function_template_by_its_origin() {
    let post = [(
        "main.cpp",
        "namespace a { template <class T> T id(T t) { return t; } }\nint use() { return a::«id»(1) + a::«id»<long>(2); }\n",
    )];
    assert_eq!(run(&case(&post, &[Named("a::id"), Named("a::id")])), [Holds, Holds]);
}

// §10.2 rule 4: the key tells overloads apart.
#[test]
fn s10_2_holds_an_overload_by_its_key() {
    let post = [(
        "main.cpp",
        "int f(int);\nint f(double);\nint use() { return «f»(1) + «f»(1.0); }\n",
    )];
    assert_eq!(run(&case(&post, &[Named("f(int)"), Named("f(double)")])), [Holds, Holds]);
}

// ---------------------------------------------------------------------
// Test group 2: fails (§10.2 rule 5, second point).
// ---------------------------------------------------------------------

// The name resolves to another overload than the one meant.
#[test]
fn s10_2_fails_another_overload() {
    let post = [("main.cpp", "int f(int);\nint f(double);\nint use() { return «f»(1); }\n")];
    let got = run(&case(&post, &[Named("f(double)")]));
    let (found, name) = fails(&got[0]).expect("fails");
    assert_eq!(found, Some(key_of(&post, "f(int)")));
    assert_eq!(name, "f(int)");
}

// A sibling spelling captured by a function of the same name in a nearer
// scope: the hazard a fully qualified name avoids.
#[test]
fn s10_2_fails_a_nearer_function_of_the_same_name() {
    let post = [(
        "main.cpp",
        "namespace lib { int g(int); }\nnamespace app { int g(int); int use() { return «g»(1); } }\n",
    )];
    let got = run(&case(&post, &[Named("lib::g(int)")]));
    let (found, name) = fails(&got[0]).expect("fails");
    assert_eq!(found, Some(key_of(&post, "app::g(int)")));
    assert_eq!(name, "app::g(int)");
}

// The name resolves to a namespace-scope variable.
#[test]
fn s10_2_fails_a_namespace_scope_variable() {
    let post = [(
        "main.cpp",
        "namespace lib { int v(); }\nnamespace app { int v; int use() { return «v»; } }\n",
    )];
    let got = run(&case(&post, &[Named("lib::v()")]));
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key_of(&post, "app::v")));
}

// The name resolves to an entity without a key (a local variable):
// `found` is absent; the name is the host's.
#[test]
fn s10_2_fails_an_entity_without_a_key() {
    let post = [("main.cpp", "int v();\nint use() { int v = 1; return «v»; }\n")];
    let got = run(&case(&post, &[Named("v()")]));
    let (found, name) = fails(&got[0]).expect("fails");
    assert_eq!(found, None);
    assert!(name.ends_with('v'), "{name}");
}

// The name resolves to another entity in a unit that is declared
// changed: the check does not look at declared changes (§10.2 last
// paragraph; `spec/api.md` §7.2 rule 5). The function has no knowledge
// of `Declared` at all, so this is the plain failure in a function a
// transform would declare changed: its body is what changed.
#[test]
fn s10_2_fails_in_a_unit_declared_changed() {
    let post = [(
        "main.cpp",
        "namespace lib { int g(int); }\nnamespace app { int g(long); int changed() { return «g»(1L); } }\n",
    )];
    let got = run(&case(&post, &[Named("lib::g(int)")]));
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key_of(&post, "app::g(long)")));
}

// ---------------------------------------------------------------------
// Test group 3: under a rename rule (`spec/edits.md` §9.3). The
// obligation's key is the pre-image's; the post-image names the entity
// by its new identifier; the rules map its key back.
// ---------------------------------------------------------------------

#[test]
fn s10_2_rename_a_function() {
    let pre = [("main.cpp", "int f(int);\nint use() { return f(1); }\n")];
    let post = [("main.cpp", "int g(int);\nint use() { return «g»(1); }\n")];
    let c = Case { pre: Some(&pre), renames: &[("f(int)", "g")], ..case(&post, &[Named("f(int)")]) };
    assert_eq!(run(&c), [Holds]);
    // Without the rules, the new name's key is another key: fails.
    let c = Case { pre: Some(&pre), ..case(&post, &[Named("f(int)")]) };
    assert!(fails(&run(&c)[0]).is_some());
}

#[test]
fn s10_2_rename_a_variable() {
    let pre = [("main.cpp", "namespace n { int v; }\nint use() { return n::v; }\n")];
    let post = [("main.cpp", "namespace n { int w; }\nint use() { return n::«w»; }\n")];
    let c = Case { pre: Some(&pre), renames: &[("n::v", "w")], ..case(&post, &[Named("n::v")]) };
    assert_eq!(run(&c), [Holds]);
}

#[test]
fn s10_2_rename_a_member_function_through_an_object() {
    let pre = [("main.cpp", "struct S { int m(); };\nint use(S s, S *p) { return s.m() + p->m(); }\n")];
    let post =
        [("main.cpp", "struct S { int n(); };\nint use(S s, S *p) { return s.«n»() + p->«n»(); }\n")];
    let c = Case {
        pre: Some(&pre),
        renames: &[("S::m()", "n")],
        ..case(&post, &[Named("S::m()"), Named("S::m()")])
    };
    assert_eq!(run(&c), [Holds, Holds]);
}

#[test]
fn s10_2_rename_a_qualified_name() {
    let pre = [("main.cpp", "namespace a { namespace b { int f(); } }\nint use() { return a::b::f(); }\n")];
    let post =
        [("main.cpp", "namespace a { namespace b { int h(); } }\nint use() { return a::b::«h»(); }\n")];
    let c = Case { pre: Some(&pre), renames: &[("a::b::f()", "h")], ..case(&post, &[Named("a::b::f()")]) };
    assert_eq!(run(&c), [Holds]);
}

// A member of a class template, renamed: the reference is to the member
// of the instantiation; its origin is the pattern's member, whose mapped
// key is the pre-image's (the explicit-instantiation question of the
// handback of WP-087, `spec/landed/verify/WP-096.md`).
#[test]
fn s10_2_rename_a_member_of_a_class_template() {
    let pre = [(
        "main.cpp",
        "template <class T> struct S { int m() { return 1; } };\ntemplate struct S<long>;\nint use() { return S<int>().m() + S<long>().m(); }\n",
    )];
    let post = [(
        "main.cpp",
        "template <class T> struct S { int n() { return 1; } };\ntemplate struct S<long>;\nint use() { return S<int>().«n»() + S<long>().«n»(); }\n",
    )];
    let c = Case {
        pre: Some(&pre),
        renames: &[("S::m()", "n")],
        ..case(&post, &[Named("S::m()"), Named("S::m()")])
    };
    assert_eq!(run(&c), [Holds, Holds]);
}

// A function template renamed: the specialisation's origin is the
// template, mapped.
#[test]
fn s10_2_rename_a_function_template() {
    let pre = [("main.cpp", "template <class T> T tf(T t) { return t; }\nint use() { return tf(1); }\n")];
    let post = [("main.cpp", "template <class T> T tg(T t) { return t; }\nint use() { return «tg»(1); }\n")];
    let c = Case { pre: Some(&pre), renames: &[("tf", "tg")], ..case(&post, &[Named("tf")]) };
    assert_eq!(run(&c), [Holds]);
}

// A rename whose replacement resolves to another function (the end-to-end
// case of WP-074): `g` already exists with a better match.
#[test]
fn s10_2_rename_captured_by_an_existing_overload() {
    let pre = [("main.cpp", "int f(long);\nint g(int);\nint use() { return f(1); }\n")];
    let post = [("main.cpp", "int g(long);\nint g(int);\nint use() { return «g»(1); }\n")];
    let c = Case { pre: Some(&pre), renames: &[("f(long)", "g")], ..case(&post, &[Named("f(long)")]) };
    let got = run(&c);
    let (found, name) = fails(&got[0]).expect("fails");
    assert_eq!(found, Some(key_of(&pre, "g(int)")));
    assert_eq!(name, "g(int)");
}

// ---------------------------------------------------------------------
// Test group 4: not seen (§10.2 rule 5, third point).
// ---------------------------------------------------------------------

#[test]
fn s10_2_not_seen_in_a_skipped_group() {
    let post = [("main.cpp", "int f();\n#if 0\nint x = «f»();\n#endif\nint y = f();\n")];
    assert_eq!(run(&case(&post, &[Named("f()")])), [NotSeen]);
}

#[test]
fn s10_2_not_seen_in_an_unused_macro_argument() {
    let post = [("main.cpp", "int f();\n#define DROP(x) 0\nint y = DROP(«f»());\n")];
    assert_eq!(run(&case(&post, &[Named("f()")])), [NotSeen]);
}

// A dependent name: resolved only at instantiation, and the template is
// never instantiated here.
#[test]
fn s10_2_not_seen_in_a_dependent_name() {
    let post = [(
        "main.cpp",
        "struct A { static int f(); };\ntemplate <class T> int g() { return T::«f»(); }\n",
    )];
    assert_eq!(run(&case(&post, &[Named("A::f()")])), [NotSeen]);
}

// Rule 3 of the package: a unit parsed without the tables.
#[test]
fn s10_2_not_seen_without_the_tables() {
    let post = [("main.cpp", "int f();\nint y = «f»();\n")];
    let c = Case { tables: false, ..case(&post, &[Named("f()")]) };
    assert_eq!(run(&c), [NotSeen]);
}

// ---------------------------------------------------------------------
// Test group 5: a macro argument substituted twice (§10.2, the paragraph
// after rule 5; rule 5 of the package).
// ---------------------------------------------------------------------

#[test]
fn s10_2_twice_substituted_one_use_wrong_fails() {
    let post = [(
        "main.cpp",
        "namespace a { int f(); }\nnamespace b { int f(); }\n#define BOTH(x) (a::x() + b::x())\nint y = BOTH(«f»);\n",
    )];
    let got = run(&case(&post, &[Named("a::f()")]));
    let (found, name) = fails(&got[0]).expect("fails");
    assert_eq!(found, Some(key_of(&post, "b::f()")));
    assert_eq!(name, "b::f()");
    // The other entity: the first use is the one that fails.
    let got = run(&case(&post, &[Named("b::f()")]));
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key_of(&post, "a::f()")));
}

#[test]
fn s10_2_twice_substituted_both_right_holds() {
    let post = [(
        "main.cpp",
        "namespace a { int f(); }\n#define TWICE(x) (a::x() + a::x())\nint y = TWICE(«f»);\n",
    )];
    assert_eq!(run(&case(&post, &[Named("a::f()")])), [Holds]);
}

// ---------------------------------------------------------------------
// Test group 6: several obligations, several files.
// ---------------------------------------------------------------------

// Two obligations in one file, one in a header, and one for a path the
// unit did not read (the caller's error): `NotSeen`.
#[test]
fn s10_6_two_in_one_file_one_in_a_header_one_unread() {
    let post = [
        ("main.cpp", "#include \"h.h\"\nint use() { return «f»(1) + «f»(2.0); }\n"),
        ("h.h", "int f(int);\nint f(double);\ninline int k() { return «f»(3); }\n"),
    ];
    let unread = Obligation { file: b"other.h".to_vec(), lo: 0, hi: 1, key: key_of(&post, "f(int)") };
    let c = case(&post, &[Named("f(int)"), Named("f(double)"), Named("f(int)")]);
    let (_, got) = run_with(&c, std::slice::from_ref(&unread));
    assert_eq!(got, [Holds, Holds, Holds, NotSeen]);
}

// The result is parallel to the obligations whatever their order, and no
// obligations give no answers.
#[test]
fn s10_6_parallel_and_empty() {
    let post = [("main.cpp", "int f(int);\nint g(int);\nint use() { return «f»(1) + «g»(2); }\n")];
    let f = key_of(&post, "f(int)");
    let g = key_of(&post, "g(int)");
    let wants = [Want::Key(g), Want::Key(f)];
    let c = case(&post, &wants);
    let got = run(&c);
    assert!(fails(&got[0]).is_some() && fails(&got[1]).is_some(), "{got:?}");
    let wants = [Want::Key(f), Want::Key(g)];
    let c = case(&post, &wants);
    assert_eq!(run(&c), [Holds, Holds]);
}
