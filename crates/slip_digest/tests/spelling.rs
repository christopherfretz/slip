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
use slip_digest::spelling::{
    Obligation, Spelling, UNRESOLVED_DEPENDENT, UNRESOLVED_FAILED, check_spellings,
};
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
    marks.sort_unstable();
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
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
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
    let (keyed, rules) = pre_keys(pre, c.std, &names, c.renames);
    let mut keyed = keyed.into_iter();
    let mut obligations: Vec<Obligation> = marks
        .into_iter()
        .zip(c.wants)
        .map(|((file, lo, hi), w)| {
            let key = match w {
                Named(_) => keyed.next().expect("a key per name"),
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

/// What a name Kiln records without a referent (dependent) fails with.
fn dependent() -> (Option<Digest>, String) {
    (None, UNRESOLVED_DEPENDENT.to_owned())
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
    let post =
        [("main.cpp", "int f(int);\nint f(double);\nint use() { return «f»(1) + «f»(1.0); }\n")];
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
    let c =
        Case { pre: Some(&pre), renames: &[("f(int)", "g")], ..case(&post, &[Named("f(int)")]) };
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
    let pre =
        [("main.cpp", "struct S { int m(); };\nint use(S s, S *p) { return s.m() + p->m(); }\n")];
    let post = [(
        "main.cpp",
        "struct S { int n(); };\nint use(S s, S *p) { return s.«n»() + p->«n»(); }\n",
    )];
    let c = Case {
        pre: Some(&pre),
        renames: &[("S::m()", "n")],
        ..case(&post, &[Named("S::m()"), Named("S::m()")])
    };
    assert_eq!(run(&c), [Holds, Holds]);
}

#[test]
fn s10_2_rename_a_qualified_name() {
    let pre = [(
        "main.cpp",
        "namespace a { namespace b { int f(); } }\nint use() { return a::b::f(); }\n",
    )];
    let post = [(
        "main.cpp",
        "namespace a { namespace b { int h(); } }\nint use() { return a::b::«h»(); }\n",
    )];
    let c = Case {
        pre: Some(&pre),
        renames: &[("a::b::f()", "h")],
        ..case(&post, &[Named("a::b::f()")])
    };
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
    let pre =
        [("main.cpp", "template <class T> T tf(T t) { return t; }\nint use() { return tf(1); }\n")];
    let post = [(
        "main.cpp",
        "template <class T> T tg(T t) { return t; }\nint use() { return «tg»(1); }\n",
    )];
    let c = Case { pre: Some(&pre), renames: &[("tf", "tg")], ..case(&post, &[Named("tf")]) };
    assert_eq!(run(&c), [Holds]);
}

// A rename whose replacement resolves to another function (the end-to-end
// case of WP-074): `g` already exists with a better match.
#[test]
fn s10_2_rename_captured_by_an_existing_overload() {
    let pre = [("main.cpp", "int f(long);\nint g(int);\nint use() { return f(1); }\n")];
    let post = [("main.cpp", "int g(long);\nint g(int);\nint use() { return «g»(1); }\n")];
    let c =
        Case { pre: Some(&pre), renames: &[("f(long)", "g")], ..case(&post, &[Named("f(long)")]) };
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
// never instantiated here. Kiln records it without a referent: the name
// at the range denotes nothing Kiln can say, so it fails (§10.2 rule 5;
// it was `NotSeen` before fix round 1, and either stops the run).
#[test]
fn s10_2_fails_at_a_dependent_name() {
    let post = [(
        "main.cpp",
        "struct A { static int f(); };\ntemplate <class T> int g() { return T::«f»(); }\n",
    )];
    assert_eq!(fails(&run(&case(&post, &[Named("A::f()")]))[0]), Some(dependent()));
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
    let unread =
        Obligation { file: b"other.h".to_vec(), lo: 0, hi: 1, key: key_of(&post, "f(int)") };
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

// ---------------------------------------------------------------------
// Attacks: attempts to make the check answer `Holds` where the name
// denotes another entity, or where nothing at the piece was checked.
// Each says which answer §10.2 requires.
// ---------------------------------------------------------------------

/// A one-file case.
fn one(src: &str, wants: &[Want<'_>]) -> Vec<Spelling> {
    run(&case(&[("main.cpp", src)], wants))
}

fn key(src: &str, name: &str) -> Digest {
    key_of(&[("main.cpp", src)], name)
}

// A name found through a using-declaration denotes its target (Kiln's
// referent is the target, not the using-shadow: the open question of
// `spec/verify.md` "What a reference found through a using-declaration
// refers to"). Holds for the target, fails for another entity of the
// same name.
#[test]
fn attack_through_a_using_declaration() {
    let src = "namespace a { int f(); }\nnamespace b { int f(); }\nnamespace c { using a::f; int u() { return «f»(); } }\n";
    assert_eq!(one(src, &[Named("a::f()")]), [Holds]);
    let got = one(src, &[Named("b::f()")]);
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key(src, "a::f()")));
}

// A namespace alias: the qualifier names the alias, the name the entity
// in the aliased namespace.
#[test]
fn attack_through_a_namespace_alias() {
    let src = "namespace a { int f(); }\nnamespace b { int f(); }\nnamespace al = b;\nint u() { return al::«f»(); }\n";
    assert_eq!(one(src, &[Named("b::f()")]), [Holds]);
    let got = one(src, &[Named("a::f()")]);
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key(src, "b::f()")));
}

// An inline namespace: `a::f` finds `a::v2::f`; the key names the inline
// namespace, so an entity of `a` itself is another key. (The host's name,
// which the cases look entities up by, leaves the inline namespace out;
// the walk's, which `found_name` is, does not: `spec/verify.md` §6.1.)
#[test]
fn attack_an_inline_namespace() {
    let src = "namespace a { inline namespace v2 { int f(int); } int f(long); }\nint u() { return a::«f»(1); }\n";
    assert_eq!(one(src, &[Named("a::f(int)")]), [Holds]);
    let got = one(src, &[Named("a::f(long)")]);
    let (found, name) = fails(&got[0]).expect("fails");
    assert_eq!(found, Some(key(src, "a::f(int)")));
    assert_eq!(name, "a::v2::f(int)");
}

// Argument-dependent lookup: the referent is what overload resolution
// chose, not what ordinary lookup found.
#[test]
fn attack_argument_dependent_lookup() {
    let src = "namespace n { struct S {}; int g(S); }\nint g(long);\nint u() { n::S s; return «g»(s); }\n";
    assert_eq!(one(src, &[Named("n::g(n::S)")]), [Holds]);
    let got = one(src, &[Named("g(long)")]);
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key(src, "n::g(n::S)")));
}

// An overload with the same parameter list in another namespace: the key
// holds the parent.
#[test]
fn attack_same_signature_another_namespace() {
    let src =
        "namespace a { int f(int); }\nnamespace b { int f(int); }\nint u() { return b::«f»(1); }\n";
    let got = one(src, &[Named("a::f(int)")]);
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key(src, "b::f(int)")));
}

// A constructor call written as the class name: the reference is to the
// class (role type name), so an obligation for the class holds and one
// for the constructor fails.
#[test]
fn attack_a_constructor() {
    let src = "struct S { S(int); };\nS s = «S»(1);\n";
    assert_eq!(one(src, &[Named("S")]), [Holds]);
    assert!(fails(&one(src, &[Named("S::S(int)")])[0]).is_some());
}

// A conversion function named explicitly, an operator function named
// explicitly, and an operator used as an operator (no reference: not
// seen).
#[test]
fn attack_conversion_and_operators() {
    let src = "struct S { operator int(); };\nint operator+(S, S);\nint u(S s) { return s.«operator int»() + «operator+»(s, s) + (s «+» s); }\n";
    let got =
        one(src, &[Named("S::operator int()"), Named("operator+(S, S)"), Named("operator+(S, S)")]);
    assert_eq!(got, [Holds, Holds, NotSeen]);
}

// A template-id: the name range is the template's name only; an
// obligation over the whole template-id is answered by nothing.
#[test]
fn attack_a_template_id() {
    let src = "template <class T> int f(T);\nint u() { return «f»<int>(1) + «f<int>»(2); }\n";
    assert_eq!(one(src, &[Named("f"), Named("f")]), [Holds, NotSeen]);
}

// A dependent call that resolves, at instantiation, to two entities in
// two instantiations. §10.2 requires `Fails` if the instantiations'
// references answer (one is wrong), `NotSeen` if none does (a name
// resolved only at instantiation that the table does not record).
#[test]
fn attack_a_dependent_call_two_instantiations() {
    let src = "namespace x { struct A {}; int h(A); }\nnamespace y { struct B {}; int h(B); }\ntemplate <class T> int g(T t) { return «h»(t); }\nint u() { return g(x::A{}) + g(y::B{}); }\n";
    // At this pin Kiln records the dependent name once, without a
    // referent, and nothing of the instantiations: it fails, for the
    // right key and the wrong one alike.
    assert_eq!(fails(&one(src, &[Named("x::h(x::A)")])[0]), Some(dependent()));
    assert_eq!(fails(&one(src, &[Named("y::h(y::B)")])[0]), Some(dependent()));
}

// A reference in a default argument used from several places, and in a
// default member initialiser used by several objects.
#[test]
fn attack_default_argument_and_member_initialiser() {
    let src = "int f();\nint h();\nint g(int a = «f»());\nstruct S { int m = «f»(); };\nS s1, s2;\nint u() { return g() + g() + s1.m; }\n";
    assert_eq!(one(src, &[Named("f()"), Named("f()")]), [Holds, Holds]);
    let got = one(src, &[Named("h()"), Named("h()")]);
    assert!(fails(&got[0]).is_some() && fails(&got[1]).is_some(), "{got:?}");
}

// Unevaluated operands: `decltype`, `sizeof`, `noexcept`.
#[test]
fn attack_unevaluated_operands() {
    let src = "int f();\nint h();\nusing T = decltype(«f»());\nunsigned long s = sizeof(«f»());\nbool b = noexcept(«f»());\n";
    assert_eq!(one(src, &[Named("f()"), Named("f()"), Named("f()")]), [Holds, Holds, Holds]);
    let got = one(src, &[Named("h()"), Named("h()"), Named("h()")]);
    assert!(got.iter().all(|g| fails(g).is_some()), "{got:?}");
}

// A requires-expression: a non-dependent name in it is resolved at the
// definition; a dependent one is not, and fails.
#[test]
fn attack_a_requires_expression() {
    let src =
        "int f();\nint k(int);\ntemplate <class T> concept C = requires(T t) { «f»(); «k»(t); };\n";
    let got = one(src, &[Named("f()"), Named("k(int)")]);
    assert_eq!(got[0], Holds);
    assert_eq!(fails(&got[1]), Some(dependent()));
    let wrong = one(src, &[Named("k(int)"), Named("f()")]);
    assert!(fails(&wrong[0]).is_some(), "{wrong:?}");
    assert_eq!(fails(&wrong[1]), Some(dependent()));
}

// Lambda captures: a simple capture names a local (no key: fails for
// any key); an init-capture's initialiser is an expression.
#[test]
fn attack_lambda_captures() {
    let src = "int f();\nint u() { int v = 1; auto l = [«v», w = «f»()] { return v + w; }; return l(); }\n";
    let got = one(src, &[Named("f()"), Named("f()")]);
    assert_eq!(got[0], Spelling::Fails { found: None, found_name: "v".into() });
    assert_eq!(got[1], Holds);
}

// A discarded `if constexpr` branch: outside a template its names are
// resolved (seen); inside one a non-dependent name is resolved at the
// definition (seen) and a dependent one is not.
#[test]
fn attack_a_discarded_branch() {
    let src = "int f();\nint h();\nint u() { if constexpr (false) { return «f»(); } return 0; }\ntemplate <class T> int t() { if constexpr (sizeof(T) == 0) { return «f»(); } return 0; }\nint v = t<int>();\n";
    assert_eq!(one(src, &[Named("f()"), Named("f()")]), [Holds, Holds]);
    let wrong = one(src, &[Named("h()"), Named("h()")]);
    assert!(!wrong.contains(&Holds), "{wrong:?}");
}

// A function template the unit never instantiates: a non-dependent name
// in it is resolved at the definition.
#[test]
fn attack_never_instantiated() {
    let src = "int f();\nint h();\ntemplate <class T> int t(T) { return «f»(); }\n";
    assert_eq!(one(src, &[Named("f()")]), [Holds]);
    assert!(fails(&one(src, &[Named("h()")])[0]).is_some());
}

// An obligation one byte off, empty, reversed, or over two tokens
// (§10.2 rule 3: ends at `hi`, starts at or after `lo`).
#[test]
fn attack_ranges_off_by_one_empty_two_tokens() {
    let src = "int f();\nint x = 1;\nint u() { return x + f(); }\n";
    // The `f` of the call is the last `f()` of the text.
    let call = src.rfind("f()").expect("f") as u32;
    let k = key(src, "f()");
    let ob = |lo: u32, hi: u32| Obligation { file: b"main.cpp".to_vec(), lo, hi, key: k };
    let files = [("main.cpp", src)];
    let c = case(&files, &[]);
    let x = src.rfind("x + f").expect("x") as u32;
    let (_, got) = run_with(
        &c,
        &[
            ob(call, call + 1),     // exact: holds
            ob(call - 1, call + 1), // one byte before: still ends at `hi`, holds
            ob(call, call + 2),     // one byte after: nothing ends there
            ob(call + 1, call + 1), // empty
            ob(call + 1, call),     // reversed
            ob(x, call + 1),        // `x + f`: answered by `f` alone (rule 3)
            ob(call + 1, call + 2), // `(`: nothing
        ],
    );
    assert_eq!(got, [Holds, Holds, NotSeen, NotSeen, NotSeen, Holds, NotSeen]);
    // Over two tokens where the last denotes another entity: fails.
    let kx = key(src, "x");
    let (_, got) = run_with(&c, &[Obligation { key: kx, ..ob(x, call + 1) }]);
    assert!(fails(&got[0]).is_some(), "{got:?}");
}

// A header without a guard entered twice, under two spellings of its
// path, where one entry's name denotes another entity (the qualifier is a
// macro redefined in between): both references have the one range and
// subject path, and one is wrong: fails.
#[test]
fn attack_a_file_entered_twice() {
    let post = [
        (
            "main.cpp",
            "namespace a { int f(); }\nnamespace b { int f(); }\n#define NS a\n#define NAME ua\n#include \"h.h\"\n#undef NS\n#undef NAME\n#define NS b\n#define NAME ub\n#include \"d/../h.h\"\n",
        ),
        ("h.h", "inline int NAME() { return NS::«f»(); }\n"),
        ("d/keep.h", ""),
    ];
    let got = run(&case(&post, &[Named("a::f()")]));
    let (found, _) = fails(&got[0]).expect("fails");
    assert_eq!(found, Some(key_of(&post, "b::f()")));
}

// A name that is a macro expanding to one name (an object-like macro):
// the reference's range is the invocation, at tier 0; it denotes what the
// macro expands to.
#[test]
fn attack_a_macro_spelled_as_the_name() {
    let src =
        "namespace a { int f(); }\nint other();\n#define g other\nint u() { return «g»(); }\n";
    let got = one(src, &[Named("a::f()")]);
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key(src, "other()")));
    assert_eq!(one(src, &[Named("other()")]), [Holds]);
}

// A name inside a macro body (tier 2): no range, not seen, whatever it
// denotes.
#[test]
fn attack_a_name_in_a_macro_body() {
    let src = "int f();\n#define CALL() f() + 0\nint u() { return CALL(); }\n";
    let call = src.rfind("CALL").expect("CALL") as u32;
    let k = key(src, "f()");
    let files = [("main.cpp", src)];
    let c = case(&files, &[]);
    let (_, got) =
        run_with(&c, &[Obligation { file: b"main.cpp".to_vec(), lo: call, hi: call + 6, key: k }]);
    assert_eq!(got, [NotSeen]);
}

// A line splice inside the name: if Kiln records the reference with a
// range, it is the token's bytes, splice included, and the name denotes
// the entity; never `Holds` for another key.
#[test]
fn attack_a_splice_in_the_name() {
    let src = "int foo();\nint h();\nint u() { return «fo\\\no»(); }\n";
    assert_eq!(one(src, &[Named("foo()")]), [Holds]);
    assert_ne!(one(src, &[Named("h()")])[0], Holds);
}

// A universal-character-name in the name of the reference.
#[test]
fn attack_a_ucn_in_the_name() {
    let src = "int f\\u00e9();\nint h();\nint u() { return «f\\u00e9»(); }\n";
    assert_eq!(one(src, &[Named("fé()")]), [Holds]);
    let got = one(src, &[Named("h()")]);
    assert_eq!(fails(&got[0]).expect("fails").0, Some(key(src, "fé()")));
}

// A key two entities share in the post-image (a collision, digest §3.4)
// is not one entity's key: a reference to either fails, `found` absent.
// Here a rename of `o::f` to `g` leaves a declaration under the old name
// (a compatibility function), whose key is the one `g`'s maps to: the
// sibling spelling `f` resolves to the old declaration, which is not the
// entity the transform meant, though its key is the obligation's.
#[test]
fn attack_a_colliding_key() {
    let pre = [("main.cpp", "namespace o { int f(int); }\nint u() { return o::f(1); }\n")];
    let post = [(
        "main.cpp",
        "namespace o { int g(int); int f(int); }\nint u() { return o::«f»(1) + o::«g»(2); }\n",
    )];
    let wants = [Named("o::f(int)"), Named("o::f(int)")];
    let c = Case { pre: Some(&pre), renames: &[("o::f(int)", "g")], ..case(&post, &wants) };
    let got = run(&c);
    assert_eq!(fails(&got[0]).expect("fails").0, None, "{got:?}");
    assert_eq!(fails(&got[1]).expect("fails").0, None, "{got:?}");
    // Without the rule nothing collides, and `f` is `f`.
    let c = Case { pre: Some(&pre), ..case(&post, &wants) };
    let got = run(&c);
    assert_eq!(got[0], Holds);
    assert!(fails(&got[1]).is_some());
}

// Under a table `RuleTable::refused` refuses, no entity has a key: an
// answered obligation fails with `found` absent, an unanswered one is not
// seen; none holds (`spec/landed/verify/WP-096.md`).
#[test]
fn attack_a_refused_table() {
    let src =
        "template <class T> struct S {};\nS<int> si;\nint f(int);\nint u() { return f(1); }\n";
    let call = src.rfind("f(1)").expect("f") as u32;
    with_unit(&[("main.cpp", src)], "c++20", true, |unit, input| {
        let ast = input.ast;
        let mut plain = Keys::new(input);
        let spec = ast
            .decls
            .ids()
            .find(|&d| {
                ast.spec_info(d).is_some_and(|(_, s)| s.kind == SpecKind::ImplicitInstantiation)
            })
            .expect("S<int>");
        let sk = plain.key(spec).expect("S<int>'s key");
        let refused = Rule {
            parent: sk.parent,
            kind: sk.kind,
            name: slip_model::keys::Name::Identifier(b"R".to_vec()),
            disc: sk.disc,
            to_parent: sk.parent,
            to_name: sk.name.clone(),
        };
        let table = RuleTable::new(vec![refused]).expect("a table may hold it");
        assert!(table.refused().is_some());
        let k = plain.key_id(entity(&plain, input, "f(int)")).expect("f's key");
        let obs = [
            Obligation { file: b"main.cpp".to_vec(), lo: call, hi: call + 1, key: k },
            Obligation { file: b"main.cpp".to_vec(), lo: call, hi: call + 2, key: k },
        ];
        let got = check_spellings(unit, input, &table, &obs);
        assert_eq!(got[0], Spelling::Fails { found: None, found_name: "f(int)".into() });
        assert_eq!(got[1], NotSeen);
        // The same obligation holds under no rule.
        assert_eq!(check_spellings(unit, input, &RuleTable::empty(), &obs[..1]), [Holds]);
    });
}

// A unit and an input of two parses (the caller's error): every
// obligation is not seen.
#[test]
fn s10_6_unit_and_input_of_two_parses() {
    let src = "int f();\nint y = f();\n";
    let call = src.rfind("f()").expect("f") as u32;
    let k = key(src, "f()");
    let ob = [Obligation { file: b"main.cpp".to_vec(), lo: call, hi: call + 1, key: k }];
    with_unit(&[("main.cpp", src)], "c++20", true, |unit, _| {
        with_unit(&[("main.cpp", src)], "c++20", true, |_, other| {
            let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                check_spellings(unit, other, &RuleTable::empty(), &ob)
            }));
            // A debug build asserts; a release build answers `NotSeen`.
            let debug = cfg!(debug_assertions);
            match got {
                Ok(got) => assert_eq!(got, [NotSeen]),
                Err(_) => assert!(debug, "only a debug build asserts"),
            }
        });
    });
}

// The named hole of §10.7: names Kiln does not record at this pin (a
// using-declaration's name, a namespace alias's target, a
// using-directive's namespace, a type-constraint's concept, a designator,
// an explicit instantiation's target; its stage 3). Alone at a range they
// are not seen. Every role takes part (§10.2 rule 1), so the day Kiln
// records one this test fails, and the `s10_7_hole_*` tests below run.
#[test]
fn s10_7_unrecorded_names_are_not_seen() {
    let src = "namespace a { int f(); struct S { int x; }; template <class T> concept C = true; template <class T> int t(T) { return 0; } }\nnamespace b { using a::«f»; }\nnamespace al = «a»;\nusing namespace «a»;\ntemplate <a::«C» T> int k(T);\na::S s = { .«x» = 1 };\ntemplate int a::«t»<int>(int);\n";
    let got = one(
        src,
        &[Named("a::f()"), Named("a"), Named("a"), Named("a::C"), Named("a::S::x"), Named("a::t")],
    );
    assert_eq!(got, [NotSeen, NotSeen, NotSeen, NotSeen, NotSeen, NotSeen]);
}

// §10.2 rule 3 "starts at or after `lo`": bytes that are the end of a
// longer name are not answered by it.
#[test]
fn s10_2_a_suffix_of_a_name_is_not_answered() {
    let src = "int ff();\nint u() { return ff(); }\n";
    let call = src.rfind("ff()").expect("ff") as u32;
    let k = key(src, "ff()");
    let files = [("main.cpp", src)];
    let c = case(&files, &[]);
    let ob = |lo, hi| Obligation { file: b"main.cpp".to_vec(), lo, hi, key: k };
    let (_, got) = run_with(&c, &[ob(call + 1, call + 2), ob(call, call + 2)]);
    assert_eq!(got, [NotSeen, Holds]);
}

// §10.2 rule 3 "in the obligation's file": the offsets of a reference of
// one file name nothing in another.
#[test]
fn s10_2_the_file_must_be_the_obligations() {
    let post = [
        ("main.cpp", "#include \"h.h\"\nint use() { return «f»(1); }\n"),
        ("h.h", "int f(int);\n"),
    ];
    let c = case(&post, &[Named("f(int)")]);
    let (obs, got) = run_with(&c, &[]);
    assert_eq!(got, [Holds]);
    let elsewhere = Obligation { file: b"h.h".to_vec(), ..obs[0].clone() };
    let (_, got) = run_with(&c, &[elsewhere]);
    assert_eq!(got, [Holds, NotSeen]);
}

// Rule 6 of the package: `found` is that of the first answering reference
// in unit order that does not have the key.
#[test]
fn s10_4_found_is_the_first_other_reference() {
    let src = "namespace a { int f(); }\nnamespace b { int f(); }\nnamespace c { int f(); }\n#define THREE(x) (a::x() + b::x() + c::x())\nint y = THREE(«f»);\n";
    let got = one(src, &[Named("a::f()")]);
    assert_eq!(fails(&got[0]).expect("fails"), (Some(key(src, "b::f()")), "b::f()".into()));
    let got = one(src, &[Named("c::f()")]);
    assert_eq!(fails(&got[0]).expect("fails"), (Some(key(src, "a::f()")), "a::f()".into()));
}

// A user-defined literal: the call of the literal operator has no entry
// (and `ranges` gives no range beginning inside a token): not seen,
// whatever the obligation covers.
#[test]
fn attack_a_user_defined_literal() {
    let src = "unsigned long long operator\"\"_k(unsigned long long);\nunsigned long long x = «12_k» + 12«_k»;\n";
    let n = "operator\"\"_k(unsigned long long)";
    assert_eq!(one(src, &[Named(n), Named(n)]), [NotSeen, NotSeen]);
}

// An operator function named explicitly whose last token is `>>`: the
// name span is Kiln's, ending where its token ends (the split-`>`
// refusal is `ranges::file_range`'s, which the check does not call).
#[test]
fn attack_operator_shift() {
    let src = "struct S {};\nint operator>>(S, int);\nint operator<<(S, int);\nint u(S s) { return «operator>>»(s, 1); }\n";
    assert_eq!(one(src, &[Named("operator>>(S, int)")]), [Holds]);
    assert!(fails(&one(src, &[Named("operator<<(S, int)")])[0]).is_some());
}

// Inside a class template, a non-dependent name is resolved once at the
// definition; two instantiations add no second reference at its range.
#[test]
fn attack_a_class_template_instantiated_twice() {
    let src = "int f();\nint h();\ntemplate <class T> struct W { int m() { return «f»(); } };\nint u() { return W<int>().m() + W<long>().m(); }\n";
    assert_eq!(one(src, &[Named("f()")]), [Holds]);
    assert!(fails(&one(src, &[Named("h()")])[0]).is_some());
}

// The same collision where both declarations are already indeterminate
// for another cause (a reflection value as a template argument is
// unsupported, digest §12): a unit keeps the first cause met, so the
// collision is found only with the causes put aside
// (`spec/landed/verify/WP-096.md` decision 1).
#[test]
fn attack_a_colliding_key_already_indeterminate() {
    let a = "template <auto V> struct A;\n";
    let body = "{ A<^^int> *x = nullptr; return x != nullptr; }";
    let pre = format!("{a}namespace o {{ int f(int) {body} }}\nint u() {{ return o::f(1); }}\n");
    let post = format!(
        "{a}namespace o {{ int g(int) {body} int f(int) {body} }}\nint u() {{ return o::«f»(1); }}\n"
    );
    let pre = [("main.cpp", pre.as_str())];
    let post = [("main.cpp", post.as_str())];
    let wants = [Named("o::f(int)")];
    let c = Case {
        pre: Some(&pre),
        renames: &[("o::f(int)", "g")],
        std: "c++26",
        ..case(&post, &wants)
    };
    let got = run(&c);
    assert_eq!(fails(&got[0]).expect("fails").0, None, "{got:?}");
}
