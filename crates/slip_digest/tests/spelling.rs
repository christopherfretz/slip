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

mod common;

use slip_digest::query::Keys;
use slip_digest::spelling::{
    Obligation, Spelling, UNRESOLVED_DEPENDENT, UNRESOLVED_FAILED, check_spellings,
};
use slip_digest::{Input, ReadOnly, ReadOnlyFiles};
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::source::FileId;
use slip_model::Digest;
use slip_model::keys::{Rule, RuleTable};
use slip_model::transform::Ownership;

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
    /// The post-image's read-only files (`spec/verify.md` §2.1): the
    /// run's ownership, as the verify job gives it to the walk and the
    /// check. Empty: every file owned.
    read_only: &'a [&'a str],
}

const fn case<'a>(post: &'a [(&'a str, &'a str)], wants: &'a [Want<'a>]) -> Case<'a> {
    Case { post, pre: None, renames: &[], wants, tables: true, std: "c++20", read_only: &[] }
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
        let files = read_only_files(input, c.read_only);
        let got = check_spellings(unit, input, &table, ReadOnly::files(&files), &obligations);
        // WP-110: with an empty read-only list every answer is what it
        // was before ownership, with every file owned.
        if c.read_only.is_empty() {
            let all = check_spellings(unit, input, &table, ReadOnly::NO_FILE, &obligations);
            assert_eq!(got, all, "an empty read-only list is every file owned");
        }
        got
    });
    assert_eq!(got.len(), obligations.len(), "parallel to the obligations");
    (obligations, got)
}

/// The subject root of a case's files, for the run's ownership.
const ROOT: &str = "/case";

/// The read-only files of `input` as the verify job decides them
/// (`ReadOnlyFiles::new` over an `Ownership` in subject paths): the
/// case's files under [`ROOT`] owned, those of `listed` read-only.
fn read_only_files(input: &Input<'_>, listed: &[&str]) -> ReadOnlyFiles {
    let mut readonly: Vec<Vec<u8>> =
        listed.iter().map(|n| format!("{ROOT}/{n}").into_bytes()).collect();
    readonly.sort();
    let o = Ownership { owned: vec![ROOT.as_bytes().to_vec()], readonly, rebuilt: Vec::new() };
    let subject = |f| {
        let p = (input.subject_path)(f);
        if p.is_empty() { p } else { [format!("{ROOT}/").as_bytes(), &p].concat() }
    };
    ReadOnlyFiles::new(Some(&o), input.sm, &subject)
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
        let got = check_spellings(unit, input, &table, ReadOnly::NO_FILE, &obs);
        assert_eq!(got[0], Spelling::Fails { found: None, found_name: "f(int)".into() });
        assert_eq!(got[1], NotSeen);
        // The same obligation holds under no rule.
        let none = RuleTable::empty();
        assert_eq!(check_spellings(unit, input, &none, ReadOnly::NO_FILE, &obs[..1]), [Holds]);
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
                check_spellings(unit, other, &RuleTable::empty(), ReadOnly::NO_FILE, &ob)
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

// The names of §10.7 Kiln did not record before its WP-1932 stage 3 (a
// using-declaration's name, a namespace alias's target, a
// using-directive's namespace, a type-constraint's concept, a designator,
// an explicit instantiation's target): recorded with a name range since,
// each alone at a range holds. Every role takes part (§10.2 rule 1).
#[test]
fn s10_7_names_recorded_since_stage_3_are_seen() {
    let src = "namespace a { int f(); struct S { int x; }; template <class T> concept C = true; template <class T> int t(T) { return 0; } }\nnamespace b { using a::«f»; }\nnamespace al = «a»;\nusing namespace «a»;\ntemplate <a::«C» T> int k(T);\na::S s = { .«x» = 1 };\ntemplate int a::«t»<int>(int);\n";
    let got = one(
        src,
        &[Named("a::f()"), Named("a"), Named("a"), Named("a::C"), Named("a::S::x"), Named("a::t")],
    );
    assert_eq!(got, [Holds, Holds, Holds, Holds, Holds, Holds]);
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

// ---------------------------------------------------------------------
// Fix round 1 (`spec/landed/verify/WP-096.md`).
// ---------------------------------------------------------------------

use slip_kiln::written::Role;

/// The roles of the names Kiln records whose range is exactly `[lo, hi)`
/// of `main.cpp` in `src` (marks removed).
fn roles_at(src: &str, lo: u32, hi: u32) -> Vec<Role> {
    let text = unmark(src).0;
    with_unit(&[("main.cpp", text.as_str())], "c++20", true, |unit, _| {
        slip_kiln::written::written_refs(unit)
            .into_iter()
            .filter(|r| {
                slip_kiln::ranges::file_range_of_span(unit.sm(), r.name)
                    .is_some_and(|f| (f.lo, f.hi) == (lo, hi))
            })
            .map(|r| r.role)
            .collect()
    })
}

/// One role Kiln records: `role_use` (with `@` for the name) names `a`
/// in role `role`; `b` is another entity of the same name, named by
/// `int o = b::@;`. The use alone **answers** (holds for `a`'s key, fails
/// for `b`'s), and one macro argument used in both places **vetoes**
/// `b`'s key with `a` found (§10.2 rules 1 and 5).
fn role_answers_and_vetoes(role: Role, prelude: &str, role_use: &str, a: &str, b: &str) {
    let alone = format!("{prelude}{}\n", role_use.replace('@', "«N»"));
    let (lo, hi) = unmark(&alone).1[0];
    assert_eq!(roles_at(&alone, lo, hi), [role], "{alone}");
    let a_key = key(&alone, a);
    assert_eq!(one(&alone, &[Named(a)]), [Holds], "{alone}");
    assert_eq!(fails(&one(&alone, &[Named(b)])[0]), Some((Some(a_key), a.to_owned())), "{alone}");
    let both =
        format!("{prelude}#define M(x) {} int o = b::x;\nM(«N»)\n", role_use.replace('@', "x"));
    let got = one(&both, &[Named(b)]);
    assert_eq!(fails(&got[0]), Some((Some(a_key), a.to_owned())), "{both}");
}

const B_N: &str = "namespace b { int N; }\n";

#[test]
fn s10_2_rule1_role_expr() {
    let p = format!("namespace a {{ int N; }}\n{B_N}");
    role_answers_and_vetoes(Role::Expr, &p, "int r = a::@;", "a::N", "b::N");
}

#[test]
fn s10_2_rule1_role_member() {
    let p = format!("struct A {{ int N; }};\n{B_N}");
    role_answers_and_vetoes(Role::Member, &p, "int r(A s) { return s.@; }", "A::N", "b::N");
}

#[test]
fn s10_2_rule1_role_nns() {
    let p = format!("namespace a {{ struct N {{ static int i; }}; }}\n{B_N}");
    role_answers_and_vetoes(Role::Nns, &p, "int r = a::@::i;", "a::N", "b::N");
}

#[test]
fn s10_2_rule1_role_type() {
    let p = format!("namespace a {{ struct N {{}}; }}\n{B_N}");
    role_answers_and_vetoes(Role::Type, &p, "a::@ r;", "a::N", "b::N");
}

#[test]
fn s10_2_rule1_role_template() {
    let p = format!("namespace a {{ template <class T> struct N {{}}; }}\n{B_N}");
    role_answers_and_vetoes(Role::Template, &p, "a::@<int> r;", "a::N", "b::N");
}

#[test]
fn s10_2_rule1_role_base() {
    let p = format!("namespace a {{ struct N {{}}; }}\n{B_N}");
    role_answers_and_vetoes(Role::Base, &p, "struct R : a::@ {};", "a::N", "b::N");
}

#[test]
fn s10_2_rule1_role_mem_init() {
    role_answers_and_vetoes(
        Role::MemInit,
        B_N,
        "struct R { int N; R() : @(1) {} };",
        "R::N",
        "b::N",
    );
}

// Kiln re-roles a name whose target is a concept to `Concept`
// (`kiln_sema/src/written_refs.rs:54-55`): it takes part (F1).
#[test]
fn s10_2_rule1_role_concept() {
    let p = format!("namespace a {{ template <class T> concept N = true; }}\n{B_N}");
    role_answers_and_vetoes(Role::Concept, &p, "static_assert(a::@<int>);", "a::N", "b::N");
}

// F1, the reviewer's P1: one macro argument names the concept `a::C` and
// the variable template `b::C`; the concept's name vetoes `b::C`.
#[test]
fn f1_a_concept_name_vetoes_through_a_macro_argument() {
    let src = "namespace a { template <class T> concept C = sizeof(T) == 4; }\nnamespace b { template <class T> constexpr bool C = true; }\n#define M(x) static_assert(a::x<int>); static_assert(b::x<int>);\nM(«C»)\n";
    let got = one(src, &[Named("b::C")]);
    assert_eq!(fails(&got[0]), Some((Some(key(src, "a::C")), "a::C".to_owned())), "{got:?}");
}

// F1, the reviewer's P2: a header without a guard included in two
// namespaces, where `C` is a concept in one and a variable template in
// the other.
#[test]
fn f1_a_concept_name_vetoes_in_a_header_entered_twice() {
    let post = [
        (
            "main.cpp",
            "namespace a { template <class T> concept C = sizeof(T) == 4;\n#include \"x.h\"\n}\nnamespace b { template <class T> constexpr bool C = true;\n#include \"x.h\"\n}\n",
        ),
        ("x.h", "inline bool g() { return «C»<int>; }\n"),
    ];
    let got = run(&case(&post, &[Named("b::C")]));
    let found = fails(&got[0]).expect("fails");
    assert_eq!(found.1, "a::C", "{got:?}");
    assert!(found.0.is_some(), "{got:?}");
}

// F4, the reviewer's P14: a concept named in an expression and in a
// requires-clause, with the concept's key, holds (it was a false alarm).
#[test]
fn f4_a_concept_name_holds() {
    let src = "namespace a { template <class T> concept C = true; }\nstatic_assert(a::«C»<int>);\ntemplate <class T> requires a::«C»<T> void f(T);\n";
    assert_eq!(one(src, &[Named("a::C"), Named("a::C")]), [Holds, Holds]);
}

// Item 3: an elaborated name (`struct X s;`) is recorded today, as
// `Type` (the reviewer's P8), so it vetoes: covered by rule 1.
#[test]
fn f2_an_elaborated_name_is_recorded_as_a_type() {
    let src = "struct X {};\nint X();\n#define M(x) struct x s; int k = x();\nM(«X»)\n";
    let got = one(src, &[Named("X()")]);
    assert_eq!(fails(&got[0]), Some((Some(key(src, "X")), "X".to_owned())), "{got:?}");
}

// F2, the reviewer's P4: one macro argument is a dependent call in a
// template (which, instantiated with `a::A`, calls `a::f` by ADL) and a
// resolved call of `::f(int)`. The dependent name has no referent: it
// fails, though the resolved call has the key.
#[test]
fn f2_a_dependent_use_of_a_macro_argument_fails() {
    let src = "namespace a { struct A {}; int f(A); }\nint f(int);\n#define M(x) template <class T> int g(T t) { return x(t); } int h() { return x(1); }\nM(«f»)\nint k = g(a::A{});\n";
    assert_eq!(fails(&one(src, &[Named("f(int)")])[0]), Some(dependent()));
}

// F2: a name whose resolution failed is not in a program that compiles;
// the constant is distinct from the dependent one, so a report can say
// which.
#[test]
fn f2_the_two_unresolved_names_differ() {
    assert_ne!(UNRESOLVED_DEPENDENT, UNRESOLVED_FAILED);
}

// The named hole (§10.2 rule 5, §10.7): a use Kiln does not record, or
// records with no range, cannot veto. Each program below was a wrong
// `Holds`; each expects `Fails`. Kiln WP-1932 stage 3 closed all but the
// pasted use, which still waits for Kiln.

#[test]
#[ignore = "needs: Kiln, a range for a pasted use of a macro argument (WP-096 For Kiln)"]
fn s10_7_hole_a_pasted_use() {
    let src =
        "int f();\nnamespace b { int f_(); }\n#define M(x) (x() + b::x##_())\nint z = M(«f»);\n";
    assert!(fails(&one(src, &[Named("f()")])[0]).is_some());
}

#[test]
fn s10_7_hole_a_designator() {
    let src = "struct S { int n; };\nint n = 2;\n#define M(x) S s{.x = 1}; int k = x;\nM(«n»)\n";
    assert!(fails(&one(src, &[Named("n")])[0]).is_some());
}

#[test]
fn s10_7_hole_a_using_declaration() {
    let src = "namespace a { int f(); }\nnamespace b { int f(); }\n#define M(x) namespace c { using a::x; } int k = b::x();\nM(«f»)\n";
    assert!(fails(&one(src, &[Named("b::f()")])[0]).is_some());
}

#[test]
fn s10_7_hole_a_type_constraint() {
    let src = "namespace a { template <class T> concept C = true; }\nnamespace b { template <class T> struct C {}; }\n#define M(x) template <a::x T> void f(T); b::x<int> v;\nM(«C»)\n";
    assert!(fails(&one(src, &[Named("b::C")])[0]).is_some());
}

#[test]
fn s10_7_hole_a_friend_declaration() {
    let src = "namespace a { struct F; }\nnamespace b { int F(); }\n#define M(x) struct S { friend struct a::x; }; int k = b::x();\nM(«F»)\n";
    assert!(fails(&one(src, &[Named("b::F()")])[0]).is_some());
}

#[test]
fn s10_7_hole_an_explicit_instantiation() {
    let src = "namespace a { template <class T> int t(T) { return 0; } }\nnamespace b { int t(int); }\n#define M(x) template int a::x<int>(int); int k = b::x(1);\nM(«t»)\n";
    assert!(fails(&one(src, &[Named("b::t(int)")])[0]).is_some());
}

// F3, the reviewer's P12: a real file whose comment holds an invalid
// byte, which Kiln replaces (three bytes for one): Kiln's offsets after it
// are not the file's. The reference of `g` at Kiln's offset would land on
// the bytes of `f`; the file is not clean, so nothing answers.
#[test]
fn f3_a_recoded_file_on_disk_is_not_seen() {
    use slip_kiln::{ParseOptions, parse_observer_with};
    use slip_model::{Command, FileRef, Language, Observer, Origin, Phase, Status};
    let bytes = b"// \xff\nint g(int);\nint f;\nint u() { return g(f); }\n";
    let dir = common::work_root().join(format!("wp096-recoded-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    std::fs::write(dir.join("main.cpp"), bytes).unwrap();
    let call = bytes.windows(4).position(|w| w == b"g(f)").unwrap() as u32;
    let argv =
        ["clang++", "-std=c++20", "main.cpp"].iter().map(|s| s.as_bytes().to_vec()).collect();
    let observer = Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.to_str().unwrap().as_bytes().to_vec(),
            argv,
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let mut options = ParseOptions::default();
    options.written = true;
    let (shard, out) = parse_observer_with(&observer, Phase::Map, 1, options, |unit| {
        let sm = unit.sm();
        let path = |f: FileId| -> Vec<u8> {
            if sm.file_name(f).ends_with("main.cpp") { b"main.cpp".to_vec() } else { Vec::new() }
        };
        let input = Input { ast: unit.ast(), sm, subject_path: &path };
        let mut keys = Keys::new(&input);
        let g = entity(&keys, &input, "g(int)");
        let f = entity(&keys, &input, "f");
        let (gk, fk) = (keys.key_id(g).unwrap(), keys.key_id(f).unwrap());
        let ob = |lo, hi, key| Obligation { file: b"main.cpp".to_vec(), lo, hi, key };
        let obligations =
            [ob(call + 2, call + 3, gk), ob(call + 2, call + 3, fk), ob(call, call + 1, gk)];
        check_spellings(unit, &input, &RuleTable::empty(), ReadOnly::NO_FILE, &obligations)
    })
    .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    assert_eq!(out.unwrap(), [NotSeen, NotSeen, NotSeen]);
}

// A collision of a key no unit has (`keys::mark_mapped_collisions` marks
// the order unit): two functions declared only by block-scope `extern`
// declarations in templates never instantiated (no unit, digest §2.2),
// which the rename's rule gives one key: `g`, renamed from `h`, and an
// `h` the post-image declares. The obligation has that key.
#[test]
fn s3_4_a_colliding_key_no_unit_has() {
    let pre = [("main.cpp", "template <class T> int t() { extern int h(); return h(); }\n")];
    let post = [(
        "main.cpp",
        "template <class T> int t() { extern int g(); return «g»(); }\ntemplate <class T> int s() { extern int h(); return h(); }\n",
    )];
    let wants = [Named("h()")];
    let c = Case { pre: Some(&pre), renames: &[("h()", "g")], ..case(&post, &wants) };
    assert_eq!(fails(&run(&c)[0]).expect("fails").0, None);
    // The same with the order unit already indeterminate for another
    // cause (namespaces nested past the order walk's depth limit,
    // `order/depth`): the order unit keeps its first cause, so the
    // collision is seen only with that cause put aside.
    let deep = format!("namespace {} {{}}\n", vec!["n"; 1100].join("::"));
    let other = format!("{deep}{}", post[0].1);
    let other = [("main.cpp", other.as_str())];
    let c = Case { pre: Some(&pre), renames: &[("h()", "g")], ..case(&other, &wants) };
    assert_eq!(fails(&run(&c)[0]).expect("fails").0, None);
    // Without the other `h` nothing collides, and `g` is the old `h`.
    let alone = [("main.cpp", "template <class T> int t() { extern int g(); return «g»(); }\n")];
    let c = Case { pre: Some(&pre), renames: &[("h()", "g")], ..case(&alone, &wants) };
    assert_eq!(run(&c), [Holds]);
}

// ---------------------------------------------------------------------
// Ownership (WP-110, `spec/verify.md` §2.1, §10.2 rule 4): keys are
// computed under the run's read-only files.
// ---------------------------------------------------------------------

/// The dependency `dep.h` (read-only) keeps `legacy::f(long)` and opens
/// `acme` with `g`; the owned `legacy` (`f(int)`, `g`) moved to `acme`
/// (`relocate legacy => :: acme`, the rename that undoes it).
const OWN_DEP: &str = "namespace legacy { int f(long); }\nnamespace acme { int g(); }\n";
const OWN_PRE: &str = "#include \"dep.h\"\nnamespace legacy { int f(int); int g(); }\nint u() { return legacy::f(1) + legacy::g(); }\n";
const OWN_POST: &str = "#include \"dep.h\"\nnamespace acme { int f(int); int g(); }\nint u() { return legacy::«f»(1) + acme::«f»(1) + acme::«g»(); }\n";

fn own_case<'a>(
    post: &'a [(&'a str, &'a str)],
    wants: &'a [Want<'a>],
    ro: &'a [&'a str],
) -> Case<'a> {
    const PRE: &[(&str, &str)] = &[("main.cpp", OWN_PRE), ("dep.h", OWN_DEP)];
    Case { pre: Some(PRE), renames: &[("legacy", "acme")], read_only: ro, ..case(post, wants) }
}

// A use in an owned file that resolves, after the edit, to the
// dependency's `legacy::f(long)`, with the key the owned `legacy::f(int)`
// has mapped: fails, and found is the dependency's own key, unmapped.
// `acme::f`, the owned entity the piece named: holds.
#[test]
fn s2_1_a_use_resolving_to_a_dependency_entity() {
    let post = [("main.cpp", OWN_POST), ("dep.h", OWN_DEP)];
    let dep_f = key_of(&post, "legacy::f(long)");
    let wants = [Named("legacy::f(int)"), Named("legacy::f(int)"), Named("legacy::g()")];
    let got = run(&own_case(&post, &wants, &["dep.h"]));
    assert_eq!(fails(&got[0]), Some((Some(dep_f), "legacy::f(long)".to_owned())), "{got:?}");
    assert_eq!(got[1], Holds);
}

// `acme::g` is declared in the owned file and in the read-only one: a
// dependency entity (§2.1), which no rule maps. Its obligation under its
// own (unmapped) key holds; under the mapped key of the owned
// `legacy::g` it fails, found the unmapped key. With every file owned
// (what a check not given the run's ownership would do) the mapped key
// would hold: the wrong answer.
#[test]
fn s2_1_an_entity_declared_in_an_owned_and_a_read_only_file() {
    let post = [("main.cpp", OWN_POST), ("dep.h", OWN_DEP)];
    let dep_g = key_of(&post, "acme::g()");
    let f = Named("legacy::f(int)");
    let wants = [f, f, Want::Key(dep_g)];
    let got = run(&own_case(&post, &wants, &["dep.h"]));
    assert_eq!(got[2], Holds, "{got:?}");
    let wants = [f, f, Named("legacy::g()")];
    let got = run(&own_case(&post, &wants, &["dep.h"]));
    assert_eq!(fails(&got[2]), Some((Some(dep_g), "acme::g()".to_owned())), "{got:?}");
    // Every file owned: `acme::g` is mapped, and the mapped key holds.
    let got = run(&own_case(&post, &wants, &[]));
    assert_eq!(got[2], Holds, "{got:?}");
}

// The collision marking runs under the same ownership as the keys: here
// a key no unit has collides (two block-scope `extern` functions, `g`
// renamed from `h`, beside another `h`), so a key is colliding unless
// some unit has it, and the dependency entity `acme::r` (declared in the
// owned file and in the read-only one) has a unit under its own,
// unmapped key. Marked with every file owned, its unit would have the
// mapped key, and its own key would read as a collision.
#[test]
fn s2_1_collisions_are_marked_under_the_same_ownership() {
    let dep = "namespace acme { int r(); }\n";
    let pre = [
        (
            "main.cpp",
            "#include \"dep.h\"\ntemplate <class T> int t() { extern int h(); return h(); }\nnamespace legacy { int r(); }\n",
        ),
        ("dep.h", dep),
    ];
    let post = [
        (
            "main.cpp",
            "#include \"dep.h\"\ntemplate <class T> int t() { extern int g(); return g(); }\ntemplate <class T> int s() { extern int h(); return h(); }\nnamespace acme { int r(); }\nint v() { return acme::«r»(); }\n",
        ),
        ("dep.h", dep),
    ];
    let dep_r = key_of(&post, "acme::r()");
    let wants = [Want::Key(dep_r)];
    let c = Case {
        pre: Some(&pre),
        renames: &[("h()", "g"), ("legacy", "acme")],
        read_only: &["dep.h"],
        ..case(&post, &wants)
    };
    assert_eq!(run(&c), [Holds]);
}

// ---------------------------------------------------------------------
// WP-141 A5: a piece that names a namespace with two view keys
// (`spec/digest.md` §3.4 "A reference to a namespace"). `legacy` moved
// into `acme`, which a read-only dependency also opens; `old_dep.h`
// keeps a read-only `legacy`.
// ---------------------------------------------------------------------

const A5_OLD: &str = "namespace legacy { struct Old {}; int k(Old); int h(int); int h(long); }\n";
const A5_NEW: &str = "namespace acme { int h(int); int h(long); }\n";
const A5_READ_ONLY: &[&str] = &["old_dep.h", "new_dep.h"];

/// The A5 case: `pre` and `post` are `main.cpp` after the two includes.
fn a5_case(pre: &str, post: &str) -> Vec<Spelling> {
    let head = "#include \"old_dep.h\"\n#include \"new_dep.h\"\n";
    let pre = format!("{head}{pre}");
    let post = format!("{head}{post}");
    let pre_files = [("main.cpp", pre.as_str()), ("old_dep.h", A5_OLD), ("new_dep.h", A5_NEW)];
    let post_files = [("main.cpp", post.as_str()), ("old_dep.h", A5_OLD), ("new_dep.h", A5_NEW)];
    let marks = unmark(&post).1.len();
    let wants = vec![Named("legacy"); marks];
    run(&Case {
        post: &post_files,
        pre: Some(&pre_files),
        renames: &[("legacy", "acme")],
        wants: &wants,
        tables: true,
        std: "c++20",
        read_only: A5_READ_ONLY,
    })
}

// F7 of the design review: the qualifier of an unresolved name that now
// finds the dependency's twin. The piece's key is a view key of `acme`
// (the owned view's, `legacy`), but at an R1 site only the key R1 writes
// holds, and R1 writes the dependency view's: fails (mutation 13).
#[test]
fn wp141_a5_r1_site_fails_with_another_view_key() {
    let got = a5_case(
        "namespace legacy { int g(int); }\ntemplate <class T> int f(T t) { return legacy::h(t); }\n",
        "namespace acme { int g(int); }\ntemplate <class T> int f(T t) { return «acme»::h(t); }\n",
    );
    assert!(fails(&got[0]).is_some(), "{got:?}");
}

// The same site finding the moved overloads: R1 writes the owned view's
// key, the pre-image's `legacy`: holds.
#[test]
fn wp141_a5_r1_site_holds_with_its_key() {
    let got = a5_case(
        "namespace legacy { int g(int); int g(long); }\ntemplate <class T> int f(T t) { return legacy::g(t); }\n",
        "namespace acme { int g(int); int g(long); }\ntemplate <class T> int f(T t) { return «acme»::g(t); }\n",
    );
    assert_eq!(got, vec![Holds]);
}

// The qualifier of a resolved name: nothing is written there and the
// next piece decides, so any view key holds.
#[test]
fn wp141_a5_resolved_qualifier_holds_with_a_view_key() {
    let got = a5_case(
        "namespace legacy { int g(int); }\nint u() { return legacy::g(1); }\n",
        "namespace acme { int g(int); }\nint u() { return «acme»::g(1); }\n",
    );
    assert_eq!(got, vec![Holds]);
}

// A using-directive's and an alias's target: Kiln records both as
// written references at the pin of WP-066, so A5 checks them with the
// whole reading's key (`ns_whole_key`). Here the spelled `acme` has two
// view keys: the post-image's directive and alias are whole for the
// dependency's `acme` alone and drop the dependency's `legacy`, which the
// pre-image's `legacy` named (Addendum 3). Both fail, and the key found is
// the dependency's `acme`, not the obligation's `legacy`.
#[test]
fn wp141_a5_directive_and_alias_targets_checked_two_views_fail() {
    let got = a5_case(
        "namespace legacy { int g(int); }\nusing namespace legacy;\nnamespace L = legacy;\n",
        "namespace acme { int g(int); }\nusing namespace «acme»;\nnamespace L = «acme»;\n",
    );
    let acme = key_of(&[("new_dep.h", A5_NEW)], "acme");
    let legacy = key_of(&[("old_dep.h", A5_OLD)], "legacy");
    assert_ne!(acme, legacy);
    assert_eq!(got.len(), 2, "{got:?}");
    for g in &got {
        assert_eq!(fails(g), Some((Some(acme), "acme".to_owned())), "{got:?}");
    }
}

/// An A5 case with no dependency: an owned `legacy` moved to `acme`.
fn a5_case_alone(pre: &str, post: &str, wants: &[Want<'_>]) -> Vec<Spelling> {
    let pre_files = [("main.cpp", pre)];
    let post_files = [("main.cpp", post)];
    run(&Case {
        post: &post_files,
        pre: Some(&pre_files),
        renames: &[("legacy", "acme")],
        wants,
        tables: true,
        std: "c++20",
        read_only: &[],
    })
}

// The twin that holds: the named namespace has one view key, the
// obligation's `legacy`, and `E(legacy)` is `acme` alone (no dependency
// opens `legacy` or `acme`), so the reading is plain and its key is
// `legacy`.
#[test]
fn wp141_a5_directive_and_alias_targets_checked_one_view_hold() {
    let got = a5_case_alone(
        "namespace legacy { int g(int); }\nusing namespace legacy;\nnamespace L = legacy;\n",
        "namespace acme { int g(int); }\nusing namespace «acme»;\nnamespace L = «acme»;\n",
        &[Named("legacy"), Named("legacy")],
    );
    assert_eq!(got, vec![Holds, Holds]);
}

// A nested target spelled as one piece (`«acme::inner»`) is answered by
// the target's reference alone (`inner`, §10.2 rule 3): one obligation,
// checked with the whole reading's key of `inner`.
#[test]
fn wp141_a5_nested_target_one_piece() {
    let got = a5_case_alone(
        "namespace legacy { namespace inner { int g(int); } }\nusing namespace legacy::inner;\nnamespace L = legacy::inner;\n",
        "namespace acme { namespace inner { int g(int); } }\nusing namespace «acme::inner»;\nnamespace L = «acme::inner»;\n",
        &[Named("legacy::inner"), Named("legacy::inner")],
    );
    assert_eq!(got, vec![Holds, Holds]);
}

// The same target spelled as two pieces: `«acme»` is a qualifier
// component (role `Nns`), checked as the qualifier of a resolved name;
// `«inner»` is the target, checked as above.
#[test]
fn wp141_a5_nested_target_two_pieces() {
    let got = a5_case_alone(
        "namespace legacy { namespace inner { int g(int); } }\nusing namespace legacy::inner;\nnamespace L = legacy::inner;\n",
        "namespace acme { namespace inner { int g(int); } }\nusing namespace «acme»::«inner»;\nnamespace L = «acme»::«inner»;\n",
        &[Named("legacy"), Named("legacy::inner"), Named("legacy"), Named("legacy::inner")],
    );
    assert_eq!(got, vec![Holds, Holds, Holds, Holds]);
}

// F3 of the second review: the piece is a prefix of a qualifier whose
// last name is unresolved (`«acme»::inner::h(t)` in a template). The edit
// is correct; the piece names `acme`, whose key is the obligation's.
#[test]
fn wp141_a5_prefix_of_an_unresolved_qualifier() {
    let got = a5_case_alone(
        "namespace legacy { namespace inner { int h(int); int h(long); } }\ntemplate <class T> int f(T t) { return legacy::inner::h(t); }\n",
        "namespace acme { namespace inner { int h(int); int h(long); } }\ntemplate <class T> int f(T t) { return «acme»::inner::h(t); }\n",
        &[Named("legacy")],
    );
    assert_eq!(got, vec![Holds]);
}

// A using-declaration's scope (row 3): the shadow's entity is a member
// of the owned part: R1 writes `legacy`: holds.
#[test]
fn wp141_a5_using_declaration_scope_holds() {
    let got = a5_case(
        "namespace legacy { int g(int); }\nusing legacy::g;\n",
        "namespace acme { int g(int); }\nusing «acme»::g;\n",
    );
    assert_eq!(got, vec![Holds]);
}

// The same scope naming the dependency's twin: R1 writes the dependency
// view's key, not the pre-image's `legacy`: fails (mutation 13).
#[test]
fn wp141_a5_using_declaration_scope_fails_with_another_view_key() {
    let got = a5_case(
        "namespace legacy { int g(int); }\nusing legacy::h;\n",
        "namespace acme { int g(int); }\nusing «acme»::h;\n",
    );
    assert!(fails(&got[0]).is_some(), "{got:?}");
}
