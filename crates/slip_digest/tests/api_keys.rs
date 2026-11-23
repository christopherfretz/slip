//! `slip_digest::query::Keys` (`docs/wp/WP-087.md`): the keys a
//! transform's declarations need (`spec/api.md` §7.5 rules 1 to 3,
//! `spec/verify.md` §2.1, §2.3, §9.5, `spec/edits.md` §9.1, §9.2, §9.7).
//!
//! Tests 1, 6 and 8 run over every directory of the pair tests, both
//! sides: what `Keys` gives must be what the full walk gives, for every
//! unit. The others are snippets.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::Side;
use slip_digest::query::{Keys, NoKey, WalkUnit, walk_units};
use slip_digest::{Cause, Detail, Input, Options, Output, ReadOnly};
use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::ids::DeclId;
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};

/// Parses `files` (the first the main file) and gives `f` the input on
/// the walk's stack; `Err` when it does not parse, and the error count.
fn with_input<R: Send>(
    files: &[(&str, &str)],
    std: &str,
    args: &[&str],
    target: Option<&str>,
    f: impl FnOnce(&Input<'_>) -> R + Send,
) -> Result<(R, u32), String> {
    let snippet = slip_kiln::snippet::Snippet { files, main: files[0].0, include: &[""], std };
    let extra = slip_kiln::snippet::Extra { args, target };
    slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let (ast, sm) = (p.ast, p.sm);
        let r = common::on_walk_stack(|| {
            let path = |f: slip_kiln::kiln::source::FileId| {
                paths.get(f.index()).cloned().unwrap_or_default()
            };
            let input = Input { ast, sm, subject_path: &path };
            f(&input)
        });
        (r, p.errors)
    })
}

/// [`with_input`] over one C++20 file `main.cpp` that must parse without
/// an error.
fn snippet<R: Send>(source: &str, f: impl FnOnce(&Input<'_>) -> R + Send) -> R {
    let (r, errors) = with_input(&[("main.cpp", source)], "c++20", &[], None, f)
        .unwrap_or_else(|e| panic!("does not parse: {e}\n{source}"));
    assert_eq!(errors, 0, "{source}");
    r
}

fn digest(input: &Input<'_>, rules: &RuleTable) -> Output {
    slip_digest::digest(
        input,
        &Options {
            rules,
            read_only: ReadOnly::NO_FILE,
            detail: Detail::All,
            names: true,
            rule_use: false,
        },
    )
}

/// The identifier `d` is declared with, if it has one.
fn ident(ast: &Ast, d: DeclId) -> Option<&str> {
    match ast.names.view(ast.decls.get(d).name()) {
        DeclNameView::Identifier(s) => Some(ast.symbols().as_str(s)),
        _ => None,
    }
}

/// Every declaration named `name` of kind `kind`, in arena order.
fn decls(ast: &Ast, name: &str, kind: DeclKind) -> Vec<DeclId> {
    ast.decls
        .ids()
        .filter(|&d| ast.decls.get(d).kind() == kind && ident(ast, d) == Some(name))
        .collect()
}

/// Whether `d` was made by instantiation (a member of a specialisation,
/// or a specialisation), not written.
fn instantiated(ast: &Ast, d: DeclId) -> bool {
    ast.member_spec_info(d).is_some()
        || ast.spec_info(d).is_some_and(|(_, s)| s.kind == SpecKind::ImplicitInstantiation)
}

/// The written entities named `name` of kind `kind` (their first
/// declarations), in arena order.
fn written(ast: &Ast, name: &str, kind: DeclKind) -> Vec<DeclId> {
    let mut all: Vec<DeclId> = decls(ast, name, kind)
        .into_iter()
        .filter(|&d| !instantiated(ast, d))
        .map(|d| ast.first(d))
        .collect();
    all.sort();
    all.dedup();
    all
}

/// The one written entity named `name` of kind `kind` (its first
/// declaration).
fn one(ast: &Ast, name: &str, kind: DeclKind) -> DeclId {
    nth(ast, name, kind, 0, 1)
}

/// The `i`th of the `of` written entities named `name` of kind `kind`.
fn nth(ast: &Ast, name: &str, kind: DeclKind, i: usize, of: usize) -> DeclId {
    let all = written(ast, name, kind);
    assert_eq!(all.len(), of, "{name} ({kind:?}): {all:?}");
    all[i]
}

/// Every declaration of kind `kind`.
fn of_kind(ast: &Ast, kind: DeclKind) -> Vec<DeclId> {
    ast.decls.ids().filter(|&d| ast.decls.get(d).kind() == kind).collect()
}

/// The call operator of the `i`th closure type of the arena.
fn call_op(ast: &Ast, i: usize) -> DeclId {
    let closures: Vec<DeclId> = ast
        .decls
        .ids()
        .filter(|&d| ast.decls.record(d).is_some_and(|r| r.lambda.is_some()))
        .filter(|&d| ast.first(d) == d)
        .collect();
    let l = ast.decls.record(closures[i]).and_then(|r| r.lambda).expect("a closure");
    ast.exprs.lambdas.get(l).call_op.expect("a call operator")
}

/// The name the walk gives each key (`Output::names`).
fn names(o: &Output) -> BTreeMap<Digest, String> {
    o.names.iter().cloned().collect()
}

/// The keys of the units of the derived groups of `o`.
fn derived_keys(o: &Output) -> BTreeSet<Digest> {
    o.digests
        .groups
        .iter()
        .filter(|g| g.class == GroupClass::Derived)
        .flat_map(|g| g.units.iter().flatten().map(|u| u.key))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests 1, 6, 8: every pair test.

/// The kinds of declaration keyed by content (digest §3.3,
/// digest-kinds §5.2).
const CONTENT_KINDS: [DeclKind; 7] = [
    DeclKind::FileScopeAsm,
    DeclKind::UsingDirective,
    DeclKind::UsingEnum,
    DeclKind::Friend,
    DeclKind::StaticAssert,
    DeclKind::ConstevalBlock,
    DeclKind::TemplateParamObject,
];

#[derive(Default)]
struct Tally {
    sides: u64,
    unparsed: u64,
    units: u64,
    keyed: u64,
    content: u64,
    indeterminate: u64,
    decls: u64,
    /// Property (a): keys `derived` gave, every one the walk's.
    derived_given: u64,
    /// Property (b): units of the walk's derived groups, and those no
    /// `derived(d)` with `d` in a declared unit lists (with their names).
    derived_units: u64,
    uncovered_compiler: u64,
    uncovered_param_object: u64,
    failures: Vec<String>,
}

/// The entity the walk treats `d` as a declaration of, as Kiln's
/// tables say it (`Walk::entity`): the first declaration, of the
/// template for a template's pattern.
fn entity_oracle(ast: &Ast, d: DeclId) -> DeclId {
    ast.described_template(d).map_or_else(|| ast.first(d), |t| ast.first(t))
}

/// Whether the compiler declared entity `e`: it is a builtin, or no
/// declaration of it, nor of a class it is a member of, has a location in
/// the source.
fn compiler_declared(input: &Input<'_>, e: DeclId) -> bool {
    let ast = input.ast;
    let located =
        |d: DeclId| input.sm.offset_of(input.sm.expansion_loc(ast.decls.get(d).loc())).is_some();
    let mut x = Some(e);
    while let Some(c) = x {
        if ast.decls.builtin_id(c).is_some() {
            return true;
        }
        if ast.redecls_of(c).any(located) {
            return false;
        }
        x = ast.decls.get(c).ctx().filter(|&p| ast.decls.record(p).is_some());
    }
    true
}

/// Properties (a) and (b) of `derived` over one side, into `t`.
fn derived_properties(
    input: &Input<'_>,
    keys: &mut Keys<'_>,
    all_units: &[WalkUnit],
    out: &Output,
    t: &mut Tally,
) {
    let ast = input.ast;
    // Properties (a) and (b) of `derived` (the fix round of WP-087):
    // (a) every key `derived(d)` gives, for any declaration `d`, is a
    // key of a unit the walk writes; (b) every unit of the walk's
    // derived groups is listed by `derived(d)` for at least one `d`
    // whose unit is a declared unit of the walk.
    let walk_keys: BTreeSet<Digest> = all_units.iter().map(|u| u.key).collect();
    let declared: BTreeSet<DeclId> =
        all_units.iter().filter(|u| !u.part && !u.derived).map(|u| u.entity).collect();
    let mut covered: BTreeSet<Digest> = BTreeSet::new();
    for d in ast.decls.ids() {
        let given = keys.derived(d);
        t.derived_given += given.len() as u64;
        for k in &given {
            if !walk_keys.contains(k) {
                t.failures.push(format!("derived({d:?}) gives {k}, a key of no unit of the walk"));
            }
        }
        if keys.unit_of(d).is_some_and(|u| declared.contains(&u)) {
            covered.extend(given);
        }
    }
    // The exceptions to (b), each with its reason
    // (`spec/landed/digest/WP-087.md`): an entity the compiler
    // declares, which no declaration of the source declares (a
    // builtin, an implicit global allocation function,
    // `std::align_val_t`, `__va_list_tag` and its members); and a
    // template parameter object, which every specialisation whose
    // arguments name it shares and none is the origin of.
    let names_of = names(out);
    for u in all_units.iter().filter(|u| u.derived) {
        t.derived_units += 1;
        if covered.contains(&u.key) {
            continue;
        }
        let name = names_of.get(&u.key).cloned().unwrap_or_else(|| format!("{}", u.key));
        if ast.decls.get(u.entity).kind() == DeclKind::TemplateParamObject {
            t.uncovered_param_object += 1;
        } else if compiler_declared(input, u.entity) {
            t.uncovered_compiler += 1;
        } else {
            t.failures.push(format!("{name}: no derived(d), d in a declared unit, lists it"));
        }
    }
}

/// One side of one pair test: tests 1, 6 and 8.
fn sweep_side(dir: &Path, side: Side) -> Result<Tally, String> {
    let all: BTreeMap<String, String> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (n, std::fs::read_to_string(e.path()).unwrap_or_default())
        })
        .collect();
    let (list, head) = common::side_files(&all, side)?;
    let mut files: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let main = files.iter().position(|(n, _)| *n == "main.cpp").expect("main.cpp");
    files.swap(0, main);
    let args: Vec<&str> = head.args.iter().map(String::as_str).collect();
    let got = with_input(&files, &head.std, &args, head.target.as_deref(), |input| {
        let mut t = Tally { sides: 1, ..Tally::default() };
        let empty = RuleTable::empty();
        let all_units = walk_units(input, &empty, ReadOnly::NO_FILE);
        let units: Vec<(DeclId, Digest, Option<Cause>)> =
            all_units.iter().filter(|u| !u.part).map(|u| (u.entity, u.key, u.key_cause)).collect();
        let out = digest(input, &empty);
        let indeterminate: BTreeSet<Digest> = out.causes.iter().map(|(k, _)| *k).collect();
        let ast = input.ast;
        // Test 1: `key_id` of every unit is the walk's key. The rule: when
        // the walk's own `Walk::key` gives the unit's key with a cause,
        // `key_id` is `Indeterminate` with that cause, and `Ok` (equal to
        // the walk's key) is a failure; else it is `Ok` with the walk's
        // key, `ContentKeyed` for a content kind, or `Indeterminate` for
        // a unit the walk's output gives a cause.
        let mut keys = Keys::new(input);
        for &(e, k, cause) in &units {
            t.units += 1;
            let kind = ast.decls.get(e).kind();
            let got = keys.key_id(e);
            match (got, cause) {
                // A content-keyed unit's key is its digest: the cause is
                // its stream's.
                (Err(NoKey::ContentKeyed), _) if CONTENT_KINDS.contains(&kind) => t.content += 1,
                (Err(NoKey::Indeterminate(c)), Some(walk)) if c == walk => t.indeterminate += 1,
                (got, Some(walk)) => t.failures.push(format!(
                    "{e:?} ({kind:?}): key_id {got:?}, the walk's key has cause {walk:?}"
                )),
                (Ok(id), None) if id == k => t.keyed += 1,
                (Ok(id), None) => {
                    t.failures.push(format!("{e:?} ({kind:?}): key_id {id}, walk {k}"));
                }
                (Err(NoKey::Indeterminate(_)), None) if indeterminate.contains(&k) => {
                    t.indeterminate += 1;
                }
                (Err(n), None) => t.failures.push(format!(
                    "{e:?} ({kind:?}, {}): key_id {n:?}, walk {k}",
                    names(&out).get(&k).cloned().unwrap_or_default()
                )),
            }
        }
        derived_properties(input, &mut keys, &all_units, &out, &mut t);
        // Test 8: `key` and `key_id` refuse alike, and `key`'s id is the
        // key id; asked first, on a fresh `Keys`, and after `key_id`.
        let mut fresh = Keys::new(input);
        for &(e, _, _) in &units {
            let first = fresh.key(e);
            let id = fresh.key_id(e);
            let again = keys.key(e);
            for (which, k) in [("first", &first), ("after key_id", &again)] {
                match (k, &id) {
                    (Ok(k), Ok(id)) if k.id() == *id => {}
                    (Err(a), Err(b)) if a == b => {}
                    (k, id) => {
                        t.failures.push(format!("{e:?}: key ({which}) {k:?}, key_id {id:?}"));
                    }
                }
            }
        }
        // Test 6: `entity` agrees with the walk on every declaration: the
        // walk's tables (`entity_oracle`), and every declaration of a
        // unit's entity has that entity.
        let unit_entities: BTreeSet<DeclId> = units.iter().map(|&(e, _, _)| e).collect();
        for d in ast.decls.ids() {
            t.decls += 1;
            let e = keys.entity(d);
            if e != entity_oracle(ast, d) || keys.entity(e) != e {
                t.failures.push(format!("entity({d:?}) is {e:?}"));
            }
        }
        for &e in &unit_entities {
            for d in ast.redecls_of(e) {
                if keys.entity(d) != e {
                    t.failures.push(format!("entity({d:?}) is not its unit's {e:?}"));
                }
            }
        }
        t
    });
    match got {
        Ok((t, _)) => Ok(t),
        Err(_) => Ok(Tally { sides: 1, unparsed: 1, ..Tally::default() }),
    }
}

// Tests 1, 6, 8 (WP-087; `spec/api.md` §7.5 rule 1, `spec/verify.md`
// §9.5 "one interface, over the functions the walk uses"): over every
// directory of the pair tests, both sides, pending ones included.
#[test]
fn keys_agree_with_the_walk_on_every_pair_test() {
    let mut dirs = Vec::new();
    for root in ["differ", "equal", "demand", "indeterminate", "resolved"] {
        dirs.extend(common::dirs(root));
    }
    let work: Vec<(std::path::PathBuf, Side)> =
        dirs.iter().flat_map(|d| [(d.clone(), Side::A), (d.clone(), Side::B)]).collect();
    let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
    let results = common::pool_map(&work, threads, |(d, s)| sweep_side(d, *s));
    let mut total = Tally::default();
    for ((dir, side), r) in work.iter().zip(results) {
        let name = format!("{} {side:?}", common::rel(dir));
        match r {
            Ok(Ok(t)) => {
                total.sides += t.sides;
                total.unparsed += t.unparsed;
                total.units += t.units;
                total.keyed += t.keyed;
                total.content += t.content;
                total.indeterminate += t.indeterminate;
                total.decls += t.decls;
                total.derived_given += t.derived_given;
                total.derived_units += t.derived_units;
                total.uncovered_compiler += t.uncovered_compiler;
                total.uncovered_param_object += t.uncovered_param_object;
                total.failures.extend(t.failures.into_iter().map(|f| format!("{name}: {f}")));
            }
            Ok(Err(e)) => total.failures.push(format!("{name}: {e}")),
            Err(p) => total.failures.push(format!("{name}: panicked: {p}")),
        }
    }
    eprintln!(
        "api_keys: {} sides ({} did not parse), {} units: {} keyed, {} content, {} indeterminate; {} declarations",
        total.sides,
        total.unparsed,
        total.units,
        total.keyed,
        total.content,
        total.indeterminate,
        total.decls
    );
    eprintln!(
        "api_keys: derived gave {} keys; {} units of derived groups, not listed from a declared unit: {} declared by the compiler, {} template parameter objects",
        total.derived_given,
        total.derived_units,
        total.uncovered_compiler,
        total.uncovered_param_object
    );
    assert!(total.failures.is_empty(), "{} failures:\n{}", total.failures.len(), {
        total.failures.truncate(60);
        total.failures.join("\n")
    });
    assert_eq!(total.unparsed, 0, "every side parses");
    assert!(total.keyed > 1000 && total.content > 10, "the sweep is not vacuous");
}

// ---------------------------------------------------------------------------
// Test 2: `unit_of` (digest §2.1, §4.3).

#[test]
fn unit_of_is_the_innermost_unit() {
    let src = "int f(int p) { int local = p; return local; }\n\
               int v = [] { return 1; }();\n\
               struct S { int m() { int q = 0; return q; } int d = 1 + 2; };\n\
               void g() { auto l = [](int a) { int z = a; return z; }; (void)l; }\n\
               enum E { e1, e2 };\n\
               template <class T> T tf(T t) { return t; }\n\
               struct Out { struct In { int w; }; };\n";
    snippet(src, |input| {
        let ast = input.ast;
        let mut keys = Keys::new(input);
        let fun = one(ast, "f", DeclKind::Function);
        // An expression's holder that is a function: the function.
        assert_eq!(keys.unit_of(fun), Some(fun));
        // A parameter and a local variable: the function.
        assert_eq!(keys.unit_of(one(ast, "p", DeclKind::ParmVar)), Some(fun));
        assert_eq!(keys.unit_of(one(ast, "local", DeclKind::Var)), Some(fun));
        // A variable's initialiser: the closure there is the variable's.
        let var = one(ast, "v", DeclKind::Var);
        assert_eq!(keys.unit_of(call_op(ast, 0)), Some(var));
        // A member function defined in class, and what its body declares.
        let meth = one(ast, "m", DeclKind::CXXMethod);
        assert_eq!(keys.unit_of(meth), Some(meth));
        assert_eq!(keys.unit_of(one(ast, "q", DeclKind::Var)), Some(meth));
        // A default member initialiser: the field's stream holds it
        // (digest-kinds §5.5 `FieldData`), and a field is a unit.
        let field = one(ast, "d", DeclKind::Field);
        assert_eq!(keys.unit_of(field), Some(field));
        // A lambda's call operator inside a function, its parameter and
        // its local: the function.
        let gfun = one(ast, "g", DeclKind::Function);
        assert_eq!(keys.unit_of(call_op(ast, 1)), Some(gfun));
        // The closure's static invoker repeats the parameter: the invoker
        // is implicit and has a body, so it is a derived unit though its
        // class is local (digest §4.3), and holds its own.
        let [param, invoker_a] = written(ast, "a", DeclKind::ParmVar)[..] else {
            panic!("two `a`")
        };
        assert_eq!(keys.unit_of(param), Some(gfun));
        let invoker = nth(ast, "__invoke", DeclKind::CXXMethod, 1, 2);
        assert_eq!(keys.unit_of(invoker), Some(invoker));
        assert_eq!(keys.unit_of(invoker_a), Some(invoker));
        assert_eq!(keys.unit_of(one(ast, "z", DeclKind::Var)), Some(gfun));
        // An enumerator is a unit.
        let e1 = one(ast, "e1", DeclKind::EnumConstant);
        assert_eq!(keys.unit_of(e1), Some(e1));
        // A function template's parameter, and its template parameter:
        // the template.
        let tf = one(ast, "tf", DeclKind::FunctionTemplate);
        assert_eq!(keys.unit_of(one(ast, "t", DeclKind::ParmVar)), Some(tf));
        let tp = decls(ast, "T", DeclKind::TemplateTypeParm);
        assert!(!tp.is_empty());
        for tparam in tp {
            assert_eq!(keys.unit_of(tparam), Some(tf));
        }
        // A member of a nested class is its own unit.
        let inner = one(ast, "w", DeclKind::Field);
        assert_eq!(keys.unit_of(inner), Some(inner));
        // The translation unit is in no unit.
        let tu = of_kind(ast, DeclKind::TranslationUnit)[0];
        assert_eq!(keys.unit_of(tu), None);
    });
}

// ---------------------------------------------------------------------------
// Test 3: `derived` (digest §2.2, `spec/api.md` §7.5 rule 1).

#[test]
fn derived_lists_instantiations_and_resolved_units() {
    let src = "template <class T> T tf(T t) { return t; }\n\
               int a = tf(1);\n\
               long b = tf(2L);\n\
               template <class T> struct C { T m() { return T(); } int n() { return 0; } };\n\
               int c = C<int>().m();\n\
               template <class T> auto ded(T t) { return t; }\n\
               int d = ded(1);\n\
               constexpr int k = 3;\n\
               struct P { template <class U> int pm(U) { return 1; } };\n\
               int e = P().pm('c');\n\
               void plain();\n\
               template <class T> constexpr T vt = T(1);\n\
               int vx = vt<int>;\n\
               struct I { int x = 1; };\n\
               I ii;\n\
               int gl() { auto b = [](auto y) { return y; }; return b(1); }\n\
               template <class T> struct FT { friend void ftf(FT) {} friend void ftd(FT); };\n\
               FT<int> fti;\n\
               void usefr() { ftf(fti); }\n\
               template <class T> struct Sp { T t; };\n\
               template <class T> struct Sp<T *> { T *p; };\n\
               Sp<long *> spl;\n\
               template <class T> struct TD { int m = [] { return 8; }(); void h(int z = [] { return 9; }()) {} };\n\
               void usetd() { TD<int> x; x.h(); }\n";
    snippet(src, |input| {
        let ast = input.ast;
        let out = digest(input, &RuleTable::empty());
        let names = names(&out);
        let derived_units = derived_keys(&out);
        let mut keys = Keys::new(input);
        let mut of = |e: DeclId| -> BTreeSet<String> {
            let ks = keys.derived(e);
            assert!(ks.windows(2).all(|w| w[0] < w[1]), "sorted, no duplicate");
            ks.iter()
                .map(|k| {
                    assert!(derived_units.contains(k), "{k} is no derived unit of the walk");
                    names.get(k).cloned().unwrap_or_else(|| format!("{k}"))
                })
                .collect()
        };
        let set = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<BTreeSet<String>>();
        // A function template with two instantiations, and each
        // instantiation's definition (WP-045: an implicit instantiation's
        // body is written in its resolved unit).
        let tf = of(one(ast, "tf", DeclKind::FunctionTemplate));
        assert_eq!(
            tf,
            set(&[
                "tf<int>(int)",
                "tf<int>(int)::<definition>",
                "tf<long>(long)",
                "tf<long>(long)::<definition>"
            ]),
            "{tf:?}"
        );
        // A class template whose member is instantiated: the class
        // specialisation and its members; of the member, its own.
        let cls = of(one(ast, "C", DeclKind::ClassTemplate));
        assert!(cls.contains("C<int>") && cls.contains("C<int>::m()"), "{cls:?}");
        let meth = of(one(ast, "m", DeclKind::CXXMethod));
        assert!(meth.contains("C<int>::m()"), "{meth:?}");
        assert!(!meth.contains("C<int>") && !meth.iter().any(|n| n.contains("::n()")), "{meth:?}");
        // A function with a deduced return type: its instantiation.
        let ded = of(one(ast, "ded", DeclKind::FunctionTemplate));
        assert!(ded.iter().any(|n| n.starts_with("ded<int>")), "{ded:?}");
        // A resolved property: the value of a constant.
        let konst = of(one(ast, "k", DeclKind::Var));
        assert_eq!(konst, set(&["k::<value>"]), "{konst:?}");
        // A member template of a class that is no template: of the class.
        let pclass = of(one(ast, "P", DeclKind::CXXRecord));
        assert!(
            pclass.iter().any(|n| n.contains("pm<char>")) && pclass.contains("P::P()"),
            "{pclass:?}"
        );
        // A class's implicit members the walk writes are derived from the
        // class (`spec/landed/digest/WP-087.md`).
        let icls = of(one(ast, "I", DeclKind::CXXRecord));
        assert!(icls.iter().any(|n| n.starts_with("I::I(")), "{icls:?}");
        // The call operator a generic lambda instantiates, a derived unit
        // of the walk though on demand: of the function that owns the
        // closure.
        let gl = of(one(ast, "gl", DeclKind::Function));
        assert!(gl.iter().any(|n| n.contains("operator()<int>")), "{gl:?}");
        // The friends a class template's instantiation defines: of the
        // template.
        let ft = of(one(ast, "FT", DeclKind::ClassTemplate));
        // (the functions themselves, not only the content-keyed friend
        // declarations: their semantic context is the namespace, their
        // lexical one the instantiation)
        assert!(ft.contains("ftf(FT<int>)") && ft.contains("ftd(FT<int>)"), "{ft:?}");
        // An instantiation of a partial specialisation: of the partial
        // specialisation (and of the template).
        let partial =
            ast.decls.ids().find(|&d| ast.partial_spec_info(d).is_some()).expect("Sp<T *>");
        let sp = of(partial);
        assert!(sp.contains("Sp<long *>"), "{sp:?}");
        assert!(of(one(ast, "Sp", DeclKind::ClassTemplate)).contains("Sp<long *>"));
        // The default arguments of an instantiated member: of the
        // template, and of the member (a field's default member
        // initialiser is its `<default_argument 0>`).
        let td = of(one(ast, "TD", DeclKind::ClassTemplate));
        for want in ["TD<int>::m::<default_argument 0>", "TD<int>::h(int)::<default_argument 0>"] {
            assert!(td.contains(want), "{want}: {td:?}");
        }
        // (Kiln records no member link for an instantiated field: the
        // pattern's is the first of the two.)
        let field = of(written(ast, "m", DeclKind::Field)[0]);
        assert!(field.contains("TD<int>::m::<default_argument 0>"), "{field:?}");
        // A variable template: its instantiation, and the instantiation's
        // own resolved unit (its value).
        let vt = of(one(ast, "vt", DeclKind::VarTemplate));
        assert!(vt.contains("vt<int>"), "{vt:?}");
        assert!(vt.iter().any(|n| n.starts_with("vt<int>") && n.contains("<value>")), "{vt:?}");
        // Nothing derives from a plain declaration.
        assert!(of(one(ast, "plain", DeclKind::Function)).is_empty());
    });
}

// ---------------------------------------------------------------------------
// An explicit instantiation is derived (WP-095's review, F3): its units
// are in a declared group, since it is written, but they change with the
// template they instantiate. For each form, a change of one token of the
// template's body changes only `e`'s unit and units `derived(e)` lists.

/// Every unit of `o`, key to digest.
fn unit_digests(o: &Output) -> BTreeMap<Digest, Digest> {
    o.digests.groups.iter().flat_map(|g| g.units.iter().flatten()).map(|u| (u.key, u.digest)).collect()
}

#[test]
fn derived_lists_explicit_instantiations() {
    const SRC: &str = "\
constexpr int g(int x) { return x; }
template <class T> int t(T) { return g(1); }
template int t<long>(long);
template <class T> int u(T) { return g(1); }
extern template int u<short>(short);
int useu = u<short>(1);
template <class T> struct C { int m() { return g(1); } };
template int C<int>::m();
template <class T> struct D { int m() { return g(1); } };
extern template int D<long>::m();
int used = D<long>().m();
";
    // (occurrence of `g(1)` changed, the entity, the explicit
    // instantiation's unit, whether it or its `<definition>` changes: `u<short>`'s
    // explicit instantiation declaration leaves its body uninstantiated;
    // `D<long>::m`'s is instantiated by its use, an inline function)
    let cases: [(usize, &str, DeclKind, usize, &str, bool); 4] = [
        (0, "t", DeclKind::FunctionTemplate, 0, "t<long>(long)", true),
        (1, "u", DeclKind::FunctionTemplate, 0, "u<short>(short)", false),
        (2, "m", DeclKind::CXXMethod, 0, "C<int>::m()", true),
        (3, "m", DeclKind::CXXMethod, 1, "D<long>::m()", true),
    ];
    let before: Vec<(BTreeSet<Digest>, BTreeMap<Digest, String>, BTreeMap<Digest, Digest>)> =
        snippet(SRC, |input| {
            let ast = input.ast;
            let out = digest(input, &RuleTable::empty());
            let mut keys = Keys::new(input);
            cases
                .iter()
                .map(|&(_, name, kind, i, _, _)| {
                    let e = nth(ast, name, kind, i, if name == "m" { 2 } else { 1 });
                    let mut declared: BTreeSet<Digest> = keys.derived(e).into_iter().collect();
                    declared.insert(keys.key_id(e).expect("a key"));
                    (declared, names(&out), unit_digests(&out))
                })
                .collect()
        });
    for (&(at, name, _, _, inst, must), (declared, names, units)) in cases.iter().zip(&before) {
        // Occurrence `at` of `g(1)` (one per template body) becomes `g(7)`.
        let (offset, _) = SRC.match_indices("g(1)").nth(at).expect("the occurrence");
        let changed = format!("{}g(7){}", &SRC[..offset], &SRC[offset + 4..]);
        let after = snippet(&changed, |input| unit_digests(&digest(input, &RuleTable::empty())));
        let differ: BTreeSet<Digest> = units
            .iter()
            .filter(|(k, v)| after.get(k) != Some(v))
            .map(|(k, _)| *k)
            .chain(after.keys().filter(|k| !units.contains_key(k)).copied())
            .collect();
        let name_of = |k: &Digest| names.get(k).cloned().unwrap_or_else(|| format!("{k}"));
        let undeclared: Vec<String> = differ.difference(declared).map(name_of).collect();
        assert!(undeclared.is_empty(), "{name} ({inst}): changed, not derived: {undeclared:?}");
        let changed_names: BTreeSet<String> = differ.iter().map(name_of).collect();
        // The instantiation's own unit, or its `<definition>`.
        let inst_changed = changed_names.iter().any(|n| n.starts_with(inst));
        assert_eq!(inst_changed, must, "{inst}: {changed_names:?}");
        let declared_names: BTreeSet<String> = declared.iter().map(name_of).collect();
        assert!(declared_names.contains(inst), "{inst} is derived: {declared_names:?}");
    }
}

// ---------------------------------------------------------------------------
// Test 5: each `NoKey` (`spec/api.md` §7.5 rule 2).

#[test]
fn each_refusal() {
    let mut deep = String::new();
    for _ in 0..250 {
        deep.push_str("namespace n { ");
    }
    deep.push_str("int deepest; ");
    for _ in 0..250 {
        deep.push('}');
    }
    let src = format!(
        "static_assert(true);\n\
         int f(int p) {{ int local = p; return local; }}\n\
         extern \"C\" {{ }}\n\
         {deep}\n"
    );
    snippet(&src, |input| {
        let ast = input.ast;
        let mut k = Keys::new(input);
        // A content-keyed unit (digest §3.3).
        let sa = of_kind(ast, DeclKind::StaticAssert)[0];
        assert_eq!(k.unit_of(sa), Some(sa));
        assert_eq!(k.key_id(sa), Err(NoKey::ContentKeyed));
        assert_eq!(k.key(sa), Err(NoKey::ContentKeyed));
        assert_eq!(k.rename_rule(sa, b"x"), Err(NoKey::ContentKeyed));
        // A new name that is no identifier is `invalid_argument` whatever
        // the entity (`spec/api.md` §7.5 rule 3 lists it first).
        assert_eq!(k.rename_rule(sa, b"1x"), Err(NoKey::BadName));
        assert_eq!(k.rename_rule(one(ast, "p", DeclKind::ParmVar), b"a b"), Err(NoKey::BadName));
        // A parameter, a local variable, a linkage specification.
        for d in [
            one(ast, "p", DeclKind::ParmVar),
            one(ast, "local", DeclKind::Var),
            of_kind(ast, DeclKind::LinkageSpec)[0],
        ] {
            assert_eq!(k.key_id(d), Err(NoKey::NotAUnit), "{d:?}");
            assert_eq!(k.key(d), Err(NoKey::NotAUnit), "{d:?}");
        }
        // An entity whose key is indeterminate: past the walk's depth.
        let deepest = one(ast, "deepest", DeclKind::Var);
        assert_eq!(k.key_id(deepest), Err(NoKey::Indeterminate(Cause::Unaccounted("walk/depth"))));
        assert_eq!(k.key(deepest), Err(NoKey::Indeterminate(Cause::Unaccounted("walk/depth"))));
        // A namespace near the top still has its key.
        assert!(k.key_id(ast.first(decls(ast, "n", DeclKind::Namespace)[0])).is_ok());
    });
}

// Test 5: a key the walk computes with a cause (a closure in the
// signature, which the walk cannot number: digest §4.3) is
// indeterminate, not given: it may not tell the entity from another.
#[test]
fn a_key_with_a_cause_is_indeterminate() {
    let src = "void f(decltype([] {}) x);\n\
               template <class T> struct S {};\n\
               template <> struct S<decltype([] {})> {};\n";
    snippet(src, |input| {
        let ast = input.ast;
        let causes = digest(input, &RuleTable::empty()).causes.len();
        assert!(causes >= 2, "the walk makes both indeterminate: {causes}");
        let f = one(ast, "f", DeclKind::Function);
        let spec = ast
            .decls
            .ids()
            .find(|&d| {
                ast.spec_info(d).is_some_and(|(_, s)| s.kind == SpecKind::ExplicitSpecialization)
            })
            .expect("S<closure>");
        for e in [f, spec] {
            // Asked first, and after the closures.
            let mut k = Keys::new(input);
            assert!(
                matches!(k.key_id(e), Err(NoKey::Indeterminate(_))),
                "{e:?}: {:?}",
                k.key_id(e)
            );
            assert_eq!(k.key(e), k.key_id(e).map(|_| unreachable!()));
            let mut k = Keys::new(input);
            for d in ast.decls.ids() {
                let _ = k.key_id(d);
            }
            assert!(
                matches!(k.key_id(e), Err(NoKey::Indeterminate(_))),
                "{e:?}: {:?}",
                k.key_id(e)
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Tests 4, 7, 8 under rules: renames done by hand.

/// One rename: `pre` with entity `name` of kind `kind` (the first of that
/// name, for an overload) renamed to `new` by hand in `post`.
struct Rename {
    what: &'static str,
    pre: &'static str,
    post: &'static str,
    name: &'static str,
    kind: DeclKind,
    new: &'static str,
}

/// What the pre-image gives: the rule, the wrong rule (for another
/// name), the entity's key and key id, the digests.
struct Pre {
    rule: Rule,
    wrong: Rule,
    key: Key,
    id: Digest,
    derived: Vec<Digest>,
    digests: slip_model::digests::Digests,
}

/// Test 4: the post-image under the rule has the pre-image's digests, and
/// under the wrong rule it has not; test 7: `with_rules` gives the
/// renamed entity its old key id; test 8: and its old key. `Err` names
/// what fails.
fn check_rename(r: &Rename) -> Result<(), String> {
    let pre = snippet(r.pre, |input| {
        let ast = input.ast;
        let e = *written(ast, r.name, r.kind).first().expect("the entity");
        let mut k = Keys::new(input);
        Pre {
            rule: k.rename_rule(e, r.new.as_bytes()).expect("a rule"),
            wrong: k.rename_rule(e, b"other_name").expect("a rule"),
            key: k.key(e).expect("a key"),
            id: k.key_id(e).expect("a key id"),
            derived: k.derived(e),
            digests: digest(input, &RuleTable::empty()).digests,
        }
    });
    let table = RuleTable::new(vec![pre.rule.clone()]).expect("a rule table");
    let wrong = RuleTable::new(vec![pre.wrong.clone()]).expect("a rule table");
    let (same, differs, id, key) = snippet(r.post, |input| {
        let e = one(input.ast, r.new, r.kind);
        let same = digest(input, &table).digests == pre.digests;
        let differs = digest(input, &wrong).digests != pre.digests;
        let mut k = Keys::with_rules(input, &table, ReadOnly::NO_FILE);
        // WP-110 (verify §2.1, rule 1): with every file read-only the rule
        // matches nothing, and the entity keeps its post-image key.
        let unmapped = Keys::with_rules(input, &table, ReadOnly::EVERY_FILE).key_id(e);
        let plain = Keys::new(input).key_id(e);
        assert!(
            unmapped == plain && unmapped != Ok(pre.id),
            "{}: test 7 (WP-110): with_rules under EVERY_FILE maps the renamed entity",
            r.what
        );
        (same, differs, k.key_id(e), (k.key(e), k.derived(e)))
    });
    let (key, derived) = key;
    let mut errs = Vec::new();
    if !same {
        errs.push("test 4: the digests differ under the rule");
    }
    if !differs {
        errs.push("test 4: the digests are equal under a rule for another name");
    }
    if id != Ok(pre.id) {
        errs.push("test 7: with_rules gives the renamed entity another key id");
    }
    if key.as_ref().ok() != Some(&pre.key) {
        errs.push("test 8: with_rules gives the renamed entity another key");
    }
    if derived != pre.derived {
        errs.push("test 7: with_rules gives the renamed entity other derived units");
    }
    if r.what == "a class" && pre.derived.is_empty() {
        errs.push("test 7: the class's implicit copy constructor is not derived from it");
    }
    if errs.is_empty() { Ok(()) } else { Err(format!("{}: {}", r.what, errs.join("; "))) }
}

const RENAMES: [Rename; 9] = [
    Rename {
        what: "a function",
        pre: "int old_f(int x) { return x; }\nint use() { return old_f(1); }\n",
        post: "int new_f(int x) { return x; }\nint use() { return new_f(1); }\n",
        name: "old_f",
        kind: DeclKind::Function,
        new: "new_f",
    },
    Rename {
        what: "one overload",
        pre: "void o(int);\nvoid o(long);\nvoid use() { o(1); o(1L); }\n",
        post: "void p(int);\nvoid o(long);\nvoid use() { p(1); o(1L); }\n",
        name: "o",
        kind: DeclKind::Function,
        new: "p",
    },
    Rename {
        what: "a member function",
        pre: "struct S { int m(); };\nint S::m() { return 1; }\nint use(S s) { return s.m(); }\n",
        post: "struct S { int n(); };\nint S::n() { return 1; }\nint use(S s) { return s.n(); }\n",
        name: "m",
        kind: DeclKind::CXXMethod,
        new: "n",
    },
    Rename {
        what: "a class",
        pre: "struct S { S(); int x; };\nS::S() : x(1) {}\nS s;\nint use(S *p) { return p->x; }\nS copy(S c) { return c; }\n",
        post: "struct T { T(); int x; };\nT::T() : x(1) {}\nT s;\nint use(T *p) { return p->x; }\nT copy(T c) { return c; }\n",
        name: "S",
        kind: DeclKind::CXXRecord,
        new: "T",
    },
    Rename {
        what: "a variable in a namespace",
        pre: "namespace n { int v = 1; }\nint use() { return n::v; }\n",
        post: "namespace n { int w = 1; }\nint use() { return n::w; }\n",
        name: "v",
        kind: DeclKind::Var,
        new: "w",
    },
    Rename {
        what: "a namespace",
        pre: "namespace a { int v; struct Q {}; }\nint use(a::Q) { return a::v; }\n",
        post: "namespace b { int v; struct Q {}; }\nint use(b::Q) { return b::v; }\n",
        name: "a",
        kind: DeclKind::Namespace,
        new: "b",
    },
    Rename {
        what: "a function with C language linkage (verify §2.3)",
        pre: "namespace ns { extern \"C\" int cf(int); }\nint use() { return ns::cf(1); }\n",
        post: "namespace ns { extern \"C\" int cg(int); }\nint use() { return ns::cg(1); }\n",
        name: "cf",
        kind: DeclKind::Function,
        new: "cg",
    },
    Rename {
        what: "a field",
        pre: "struct S { int a; };\nint use(S s) { return s.a; }\n",
        post: "struct S { int b; };\nint use(S s) { return s.b; }\n",
        name: "a",
        kind: DeclKind::Field,
        new: "b",
    },
    Rename {
        what: "a member function with an overload left",
        pre: "struct S { int m(int); int m(long); };\nint use(S s) { return s.m(1) + s.m(1L); }\n",
        post: "struct S { int k(int); int m(long); };\nint use(S s) { return s.k(1) + s.m(1L); }\n",
        name: "m",
        kind: DeclKind::CXXMethod,
        new: "k",
    },
];

// Tests 4, 7 and 8 (verify §2.1, §2.3): every rename of `RENAMES`.
#[test]
fn rename_rule_undoes_a_rename() {
    let mut errs = Vec::new();
    for r in &RENAMES {
        if let Err(e) = check_rename(r) {
            errs.push(e);
        }
    }
    assert!(errs.is_empty(), "{}", errs.join("\n"));
}

// The renames that waited for WP-103 (a specialisation's key takes its
// template's mapped name, a member of an implicit instantiation is
// reached by its template member's rule, a decomposition takes its first
// binding's mapped name), which pass since it landed.

const AFTER_WP103: [Rename; 8] = [
    Rename {
        what: "a class template with an instantiation",
        pre: "template <class T> struct S { T t; };\nS<int> s;\nint use() { return s.t; }\n",
        post: "template <class T> struct R { T t; };\nR<int> s;\nint use() { return s.t; }\n",
        name: "S",
        kind: DeclKind::ClassTemplate,
        new: "R",
    },
    Rename {
        what: "a member function of a class template with an instantiation",
        pre: "template <class T> struct S { int m() { return 1; } };\nint use() { return S<int>().m(); }\n",
        post: "template <class T> struct S { int n() { return 1; } };\nint use() { return S<int>().n(); }\n",
        name: "m",
        kind: DeclKind::CXXMethod,
        new: "n",
    },
    Rename {
        what: "a structured binding",
        pre: "struct P { int a, b; };\nauto [x, y] = P{1, 2};\nint use() { return x + y; }\n",
        post: "struct P { int a, b; };\nauto [z, y] = P{1, 2};\nint use() { return z + y; }\n",
        name: "x",
        kind: DeclKind::Binding,
        new: "z",
    },
    Rename {
        what: "a function template with an instantiation",
        pre: "template <class T> T tf(T t) { return t; }\nint use() { return tf(1); }\n",
        post: "template <class T> T tg(T t) { return t; }\nint use() { return tg(1); }\n",
        name: "tf",
        kind: DeclKind::FunctionTemplate,
        new: "tg",
    },
    Rename {
        what: "a variable template with an instantiation",
        pre: "template <class T> constexpr T vt = T(1);\nint use() { return vt<int>; }\n",
        post: "template <class T> constexpr T vu = T(1);\nint use() { return vu<int>; }\n",
        name: "vt",
        kind: DeclKind::VarTemplate,
        new: "vu",
    },
    Rename {
        what: "a function template with an explicit specialisation",
        pre: "template <class T> int ft(T) { return 1; }\ntemplate <> int ft(char) { return 2; }\n",
        post: "template <class T> int fu(T) { return 1; }\ntemplate <> int fu(char) { return 2; }\n",
        name: "ft",
        kind: DeclKind::FunctionTemplate,
        new: "fu",
    },
    Rename {
        what: "a member function template of a class that is no template",
        pre: "struct P { template <class U> int pm(U) { return 1; } };\nint use() { return P().pm(1); }\n",
        post: "struct P { template <class U> int pn(U) { return 1; } };\nint use() { return P().pn(1); }\n",
        name: "pm",
        kind: DeclKind::FunctionTemplate,
        new: "pn",
    },
    Rename {
        what: "a class template used through class template argument deduction",
        pre: "template <class T> struct S { S(T) {} };\nS s(1);\n",
        post: "template <class T> struct R { R(T) {} };\nR s(1);\n",
        name: "S",
        kind: DeclKind::ClassTemplate,
        new: "R",
    },
];

// Tests 4, 7 and 8 for a renamed class template with an
// instantiation.
#[test]
fn rename_after_wp103_class_template_with_instantiation() {
    check_rename(&AFTER_WP103[0]).unwrap_or_else(|e| panic!("{e}"));
}

// The same for a member function of a class template
// with an instantiation.
#[test]
fn rename_after_wp103_member_of_class_template_with_instantiation() {
    check_rename(&AFTER_WP103[1]).unwrap_or_else(|e| panic!("{e}"));
}

// The same for a structured binding.
#[test]
fn rename_after_wp103_structured_binding() {
    check_rename(&AFTER_WP103[2]).unwrap_or_else(|e| panic!("{e}"));
}

// The same for a function template with an
// instantiation.
#[test]
fn rename_after_wp103_function_template_with_instantiation() {
    check_rename(&AFTER_WP103[3]).unwrap_or_else(|e| panic!("{e}"));
}

// The same for a variable template with an
// instantiation.
#[test]
fn rename_after_wp103_variable_template_with_instantiation() {
    check_rename(&AFTER_WP103[4]).unwrap_or_else(|e| panic!("{e}"));
}

// The same for a function template with an explicit
// specialisation.
#[test]
fn rename_after_wp103_function_template_with_explicit_specialisation() {
    check_rename(&AFTER_WP103[5]).unwrap_or_else(|e| panic!("{e}"));
}

// The same for a member function template of a class
// that is no template.
#[test]
fn rename_after_wp103_member_function_template() {
    check_rename(&AFTER_WP103[6]).unwrap_or_else(|e| panic!("{e}"));
}

// The same for a class template used through class
// template argument deduction (its implicit deduction guides).
#[test]
fn rename_after_wp103_class_template_with_deduction() {
    check_rename(&AFTER_WP103[7]).unwrap_or_else(|e| panic!("{e}"));
}

// A table that holds a rule `spec/verify.md` §2.1 refuses (WP-103,
// `RuleTable::refused`): the walk keys nothing under it, and `Keys`
// mirrors it, with the walk's cause, content-keyed units included.
#[test]
fn a_refused_table_keys_nothing() {
    let src = "template <class T> struct S {};\n\
               S<int> si;\n\
               int f(int p) { return p; }\n\
               static_assert(true);\n";
    snippet(src, |input| {
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
        let refused_rule = Rule {
            parent: sk.parent,
            kind: sk.kind,
            name: Name::Identifier(b"R".to_vec()),
            disc: sk.disc,
            to_parent: sk.parent,
            to_name: sk.name.clone(),
        };
        let table = RuleTable::new(vec![refused_rule]).expect("a table may hold it");
        assert!(table.refused().is_some());
        let cause = NoKey::Indeterminate(Cause::Unaccounted("key/refused-rule"));
        let mut k = Keys::with_rules(input, &table, ReadOnly::NO_FILE);
        let f = one(ast, "f", DeclKind::Function);
        let s = one(ast, "S", DeclKind::ClassTemplate);
        let sa = of_kind(ast, DeclKind::StaticAssert)[0];
        for e in [f, s, spec, sa] {
            assert_eq!(k.key_id(e), Err(cause), "{e:?}");
            assert_eq!(k.key(e), Err(cause), "{e:?}");
            assert!(k.derived(e).is_empty(), "{e:?}");
        }
        assert_eq!(k.rename_rule(f, b"g"), Err(cause));
        assert!(!plain.derived(s).is_empty(), "S<int> is derived from S without the table");
        // What has no key has none under it either.
        assert_eq!(k.key_id(one(ast, "p", DeclKind::ParmVar)), Err(NoKey::NotAUnit));
        // The walk agrees: every unit it writes under the table is
        // indeterminate.
        let out = digest(input, &table);
        let units = walk_units(input, &table, ReadOnly::NO_FILE);
        let causes: BTreeSet<Digest> = out.causes.iter().map(|(k, _)| *k).collect();
        assert!(!units.is_empty() && units.iter().all(|u| causes.contains(&u.key)));
    });
}

// verify §2.3 "An entity with C language linkage": the rule of a function
// with C language linkage keeps the parent `c_linkage` on both sides,
// whatever namespace declares it.
#[test]
fn c_linkage_rule_keeps_its_parent() {
    let src = "namespace ns { extern \"C\" int cf(int); }\n";
    snippet(src, |input| {
        let mut k = Keys::new(input);
        let cf = one(input.ast, "cf", DeclKind::Function);
        let rule = k.rename_rule(cf, b"cg").expect("a rule");
        assert_eq!(rule.parent, Parent::CLinkage);
        assert_eq!(rule.to_parent, Parent::CLinkage);
        assert_eq!(rule.name, Name::Identifier(b"cg".to_vec()));
        assert_eq!(rule.to_name, Name::Identifier(b"cf".to_vec()));
        assert_eq!(rule.disc, None, "a function with C language linkage has none");
    });
}

// The parts of a rule (verify §2.1): the new name to match, the old one
// to give, the discriminator and kind kept, the parent on both sides.
#[test]
fn rule_fields() {
    let src = "namespace a { struct S { int m(int); }; }\n";
    snippet(src, |input| {
        let ast = input.ast;
        let mut k = Keys::new(input);
        let m = one(ast, "m", DeclKind::CXXMethod);
        let key = k.key(m).expect("a key");
        let s = k.key_id(one(ast, "S", DeclKind::CXXRecord)).expect("S's key id");
        let rule = k.rename_rule(m, b"n").expect("a rule");
        assert_eq!(
            rule,
            Rule {
                parent: Parent::Unit(s),
                kind: KeyKind::Function,
                name: Name::Identifier(b"n".to_vec()),
                disc: key.disc,
                to_parent: Parent::Unit(s),
                to_name: Name::Identifier(b"m".to_vec()),
            }
        );
        assert!(key.disc.is_some());
    });
}

// `rename_rule`'s refusals (`spec/api.md` §7.5 rule 3; WP-103's rules a
// table may not contain): a specialisation and a partial specialisation
// only follow their template, a decomposition its first binding; a name
// that is no identifier; a new name that is no identifier or is the old
// one. The first are `NotRenamable` (`unsupported`), the last `BadName`
// (`invalid_argument`) (`spec/landed/digest/WP-087.md`).
#[test]
fn rename_rule_refusals() {
    let src = "template <class T> struct S {};\n\
               template <> struct S<int> {};\n\
               template <class T> struct S<T *> {};\n\
               struct P { int a, b; };\n\
               auto [x, y] = P{1, 2};\n\
               struct Q { Q(); bool operator==(const Q &) const; };\n\
               namespace { int hidden; }\n\
               int f();\n";
    snippet(src, |input| {
        let ast = input.ast;
        let mut k = Keys::new(input);
        let spec = ast
            .decls
            .ids()
            .find(|&d| {
                ast.spec_info(d).is_some_and(|(_, s)| s.kind == SpecKind::ExplicitSpecialization)
            })
            .expect("S<int>");
        let partial =
            ast.decls.ids().find(|&d| ast.partial_spec_info(d).is_some()).expect("S<T *>");
        assert_eq!(k.key(spec).map(|k| k.kind), Ok(KeyKind::Specialisation));
        assert_eq!(k.rename_rule(spec, b"R"), Err(NoKey::NotRenamable));
        assert_eq!(k.key(partial).map(|k| k.kind), Ok(KeyKind::PartialSpecialisation));
        assert_eq!(k.rename_rule(partial, b"R"), Err(NoKey::NotRenamable));
        let decomposition = of_kind(ast, DeclKind::Var)
            .into_iter()
            .find(|&d| ast.decls.is_decomposition(d))
            .expect("the decomposition");
        assert_eq!(k.key(decomposition).map(|k| k.kind), Ok(KeyKind::Decomposition));
        assert_eq!(k.rename_rule(decomposition, b"r"), Err(NoKey::NotRenamable));
        // The binding itself is renamable.
        assert!(k.rename_rule(one(ast, "x", DeclKind::Binding), b"z").is_ok());
        // Names that are no identifier.
        let ctor = of_kind(ast, DeclKind::CXXConstructor)
            .into_iter()
            .find(|&d| ast.decls.get(d).ctx().is_some_and(|c| ident(ast, c) == Some("Q")))
            .expect("Q()");
        assert_eq!(k.rename_rule(ctor, b"r"), Err(NoKey::NotRenamable));
        let op = of_kind(ast, DeclKind::CXXMethod)
            .into_iter()
            .find(|&d| matches!(ast.names.view(ast.decls.get(d).name()), DeclNameView::Operator(_)))
            .expect("operator==");
        assert_eq!(k.rename_rule(op, b"r"), Err(NoKey::NotRenamable));
        let anonymous = of_kind(ast, DeclKind::Namespace)
            .into_iter()
            .find(|&d| ident(ast, d).is_none_or(str::is_empty))
            .expect("the unnamed namespace");
        assert_eq!(k.rename_rule(anonymous, b"r"), Err(NoKey::NotRenamable));
        // New names that are no identifier, or the old one: `BadName`
        // (`invalid_argument`), judged before the entity (§7.5 rule 3).
        let f = one(ast, "f", DeclKind::Function);
        for bad in [&b""[..], b"1f", b"a-b", b"a b", b"f"] {
            assert_eq!(k.rename_rule(f, bad), Err(NoKey::BadName), "{bad:?}");
        }
        assert_eq!(k.rename_rule(ctor, b"1r"), Err(NoKey::BadName));
        assert!(k.rename_rule(f, b"_g9").is_ok());
        // The limits of the identifier check, as seen today, both failing
        // closed (`spec/landed/digest/WP-087.md`): a keyword is taken (the
        // post-image does not parse, so nothing verifies), and an
        // identifier with a universal character name or a code point
        // outside ASCII is refused.
        assert!(k.rename_rule(f, b"class").is_ok());
        assert!(k.rename_rule(f, b"and").is_ok());
        for refused in ["é", "aé", r"a\u00e9", r"\U000000e9"] {
            assert_eq!(k.rename_rule(f, refused.as_bytes()), Err(NoKey::BadName), "{refused}");
        }
    });
}

// Test 8 (`spec/edits.md` §9.1, §9.2): the key's fields as the native
// rename reads them: a parent chain through namespaces, an unnamed class
// named by a variable, a function in a linkage specification.
#[test]
fn key_fields() {
    let src = "namespace a { namespace b { void f(); } }\n\
               struct { int x; } var;\n\
               extern \"C\" void cf();\n";
    snippet(src, |input| {
        let ast = input.ast;
        let mut k = Keys::new(input);
        let a = ast.first(decls(ast, "a", DeclKind::Namespace)[0]);
        let b = ast.first(decls(ast, "b", DeclKind::Namespace)[0]);
        let f = one(ast, "f", DeclKind::Function);
        let fk = k.key(f).expect("f's key");
        assert_eq!(fk.parent, Parent::Unit(k.key_id(b).expect("b")));
        assert_eq!(fk.name, Name::Identifier(b"f".to_vec()));
        assert_eq!(fk.kind, KeyKind::Function);
        let bk = k.key(b).expect("b's key");
        assert_eq!(bk.parent, Parent::Unit(k.key_id(a).expect("a")));
        assert_eq!(k.key(a).expect("a's key").parent, Parent::Root);
        let class = of_kind(ast, DeclKind::CXXRecord)
            .into_iter()
            .find(|&d| ident(ast, d).is_none_or(str::is_empty))
            .expect("the unnamed class");
        let ck = k.key(class).expect("the class's key");
        assert_eq!(ck.name, Name::UnnamedBy(Box::new(Name::Identifier(b"var".to_vec()))));
        let cf = one(ast, "cf", DeclKind::Function);
        assert_eq!(k.key(cf).expect("cf's key").parent, Parent::CLinkage);
    });
}

// `spec/edits.md` §9.2: the rule of a follower whose key does not hold
// the identifier as its name (an unnamed class named by a renamed
// variable) is made from its `Key`: under that rule and the variable's,
// the post-image has the pre-image's digests.
#[test]
fn rule_of_an_unnamed_class_from_its_key() {
    let pre = "struct { int x; } var;\nint use() { return var.x; }\n";
    let post = "struct { int x; } val;\nint use() { return val.x; }\n";
    let (rules, digests) = snippet(pre, |input| {
        let ast = input.ast;
        let mut k = Keys::new(input);
        let var = one(ast, "var", DeclKind::Var);
        let class = of_kind(ast, DeclKind::CXXRecord)
            .into_iter()
            .find(|&d| ident(ast, d).is_none_or(str::is_empty))
            .expect("the unnamed class");
        assert_eq!(k.rename_rule(class, b"val"), Err(NoKey::NotRenamable));
        let ck = k.key(class).expect("the class's key");
        let follower = Rule {
            parent: ck.parent,
            kind: ck.kind,
            name: Name::UnnamedBy(Box::new(Name::Identifier(b"val".to_vec()))),
            disc: ck.disc,
            to_parent: ck.parent,
            to_name: ck.name.clone(),
        };
        let rules = vec![k.rename_rule(var, b"val").expect("a rule"), follower];
        (rules, digest(input, &RuleTable::empty()).digests)
    });
    let table = RuleTable::new(rules).expect("a rule table");
    let same = snippet(post, |input| digest(input, &table).digests == digests);
    assert!(same, "the digests differ under the two rules");
}

// `Keys::new` runs nothing, and a `Keys` asked in another order gives the
// same answers (the memo is the walk's).
#[test]
fn answers_do_not_depend_on_order() {
    let src = "namespace a { struct S { int m(int); void g(); }; template <class T> T t(T); }\n\
               int u = a::t(1);\n";
    snippet(src, |input| {
        let all: Vec<DeclId> = input.ast.decls.ids().collect();
        let mut fwd = Keys::new(input);
        let mut back = Keys::new(input);
        let one: Vec<_> = all.iter().map(|&d| (fwd.key_id(d), fwd.unit_of(d))).collect();
        let mut two: Vec<_> =
            all.iter().rev().map(|&d| (back.key_id(d), back.unit_of(d))).collect();
        two.reverse();
        assert_eq!(one, two);
    });
}
