//! Unit records (`spec/verify.md` §2.3 "Records", "Tests"): the records of
//! snippets (a fallback key is `records::tests`: no source makes one), two entities of one name, records under
//! rules, and, over every directory of the pair tests (pending ones
//! included), both sides: `records` is `Ok`, its ids are exactly the unit
//! keys of `digest` with detail `all`, and every record's `Key` has the
//! record's id.
//!
//! The question this file answers for the trusted base: `records` cannot
//! disturb a digest. It walks on its own (`walk::run_with`), and
//! `tests/determinism.rs` and the goldens, which do not move, show that
//! `run_with` is the walk `digest` runs.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::Side;
use slip_digest::records::{Record, records};
use slip_digest::{Detail, Input, Options, Output};
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};

/// Parses `files` (the first the main file) under `std`, and gives `f` the
/// input on the walk's stack.
fn with_input<R: Send>(
    files: &[(&str, &str)],
    std: &str,
    args: &[&str],
    f: impl FnOnce(&Input<'_>) -> R + Send,
) -> Result<(R, u32), String> {
    let snippet = slip_kiln::snippet::Snippet { files, main: files[0].0, include: &[""], std };
    let extra = slip_kiln::snippet::Extra { args, target: None };
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

/// The records of one snippet under `rules`, and its digest with detail
/// `all` under the same rules.
fn snippet(source: &str, rules: &RuleTable) -> (Vec<Record>, Output) {
    let ((recs, out), errors) = with_input(&[("main.cpp", source)], "c++20", &[], |input| {
        let recs = records(input, rules, slip_digest::ReadOnly::NO_FILE);
        let out = slip_digest::digest(
            input,
            &Options {
                rules,
                read_only: slip_digest::ReadOnly::NO_FILE,
                detail: Detail::All,
                names: true,
                rule_use: false,
            },
        );
        (recs, out)
    })
    .expect("the snippet parses");
    assert_eq!(errors, 0, "{source}");
    (recs.expect("records are Ok"), out)
}

/// The record of `name` (the only one).
fn named<'r>(recs: &'r [Record], name: &str) -> &'r Record {
    let v: Vec<&Record> = recs.iter().filter(|r| r.name == name).collect();
    assert_eq!(v.len(), 1, "records named {name}: {v:?}\nall: {recs:#?}");
    v[0]
}

/// The unit keys of an output with detail `all`, as a set.
fn unit_keys(out: &Output) -> BTreeSet<Digest> {
    out.digests.groups.iter().flat_map(|g| g.units.iter().flatten().map(|u| u.key)).collect()
}

/// What every set of records must be against its digest (§2.3 "Records",
/// `records`' contract): ids exactly the unit keys; sorted by
/// `(id, name)`, no two equal; a `Key` whose id is the record's id; a
/// name among the walk's names for that key; the indeterminate flag that
/// of a unit of that key.
fn check(recs: &[Record], out: &Output) -> Result<(), String> {
    let ids: BTreeSet<Digest> = recs.iter().map(|r| r.id).collect();
    let keys = unit_keys(out);
    if ids != keys {
        let only_r: Vec<_> = ids.difference(&keys).collect();
        let only_d: Vec<_> = keys.difference(&ids).collect();
        return Err(format!(
            "record ids are not the unit keys: records only {only_r:?}, digest only {only_d:?}"
        ));
    }
    for w in recs.windows(2) {
        if (w[0].id, &w[0].name) > (w[1].id, &w[1].name) {
            return Err(format!("not sorted by (id, name): {:?} then {:?}", w[0], w[1]));
        }
        if w[0] == w[1] {
            return Err(format!("a record twice: {:?}", w[0]));
        }
    }
    for r in recs {
        if let Some(k) = &r.key {
            if k.id() != r.id {
                return Err(format!("the Key of {} has the id {}, not {}", r.name, k.id(), r.id));
            }
            if k.kind != r.kind {
                return Err(format!(
                    "the Key of {} has the kind {:?}, not {:?}",
                    r.name, k.kind, r.kind
                ));
            }
        }
        if out.names.binary_search(&(r.id, r.name.clone())).is_err() {
            return Err(format!("the name `{}` is not the walk's for {}", r.name, r.id));
        }
        let flag = u64::from(r.indeterminate);
        let unit = out
            .digests
            .groups
            .iter()
            .filter(|g| g.class == r.class)
            .any(|g| g.units.iter().flatten().any(|u| u.key == r.id && (u.flags & 1) == flag));
        if !unit {
            return Err(format!("no unit of {} in class {:?} with flag {flag}", r.name, r.class));
        }
    }
    Ok(())
}

fn ident(s: &str) -> Name {
    Name::Identifier(s.as_bytes().to_vec())
}

// §2.3 "Records": the records of a snippet: a namespace, overloads, a
// class and a member function, a variable, the order unit; each with its
// kind, class, name and a `Key` whose id is its own.
#[test]
fn records_of_a_snippet() {
    let src = "namespace ns {\n  void f(int);\n  void f(char *);\n  struct C { void m() const; };\n}\nint x;\n";
    let (recs, out) = snippet(src, &RuleTable::empty());
    check(&recs, &out).unwrap();
    let ns = named(&recs, "ns");
    assert_eq!((ns.kind, ns.class), (KeyKind::Namespace, GroupClass::Tu));
    let key = ns.key.clone().unwrap();
    assert_eq!((key.parent, key.name.clone(), key.disc), (Parent::Root, ident("ns"), None));
    let ns_id = ns.id;
    for name in ["ns::f(int)", "ns::f(char *)"] {
        let f = named(&recs, name);
        assert_eq!((f.kind, f.class), (KeyKind::Function, GroupClass::Declared));
        let k = f.key.clone().unwrap();
        assert_eq!((k.parent, k.name.clone()), (Parent::Unit(ns_id), ident("f")));
        assert!(k.disc.is_some(), "a function's key has its signature");
    }
    assert_ne!(named(&recs, "ns::f(int)").id, named(&recs, "ns::f(char *)").id);
    let c = named(&recs, "ns::C");
    let m = named(&recs, "ns::C::m() const");
    assert_eq!(m.key.as_ref().unwrap().parent, Parent::Unit(c.id));
    let x = named(&recs, "x");
    assert_eq!((x.kind, x.key.as_ref().unwrap().parent), (KeyKind::Variable, Parent::Root));
    let order = named(&recs, "<order>");
    let want = Key { kind: KeyKind::Order, parent: Parent::Root, name: Name::Order, disc: None };
    assert_eq!(order.key, Some(want));
    assert_eq!((order.kind, order.class), (KeyKind::Order, GroupClass::Tu));
    assert!(recs.iter().all(|r| r.key.is_some()), "no fallback key here");
}

// §2.3: two entities of one name, of two kinds (a class and the alias of
// the same name): two records, two ids, the kinds that tell them apart.
#[test]
fn two_entities_of_one_name() {
    let (recs, out) =
        snippet("struct T {};\ntypedef struct T T;\nusing U = int;\n", &RuleTable::empty());
    check(&recs, &out).unwrap();
    let t: Vec<&Record> = recs.iter().filter(|r| r.name == "T").collect();
    assert_eq!(t.len(), 2, "{recs:#?}");
    assert_ne!(t[0].id, t[1].id);
    let kinds: BTreeSet<KeyKind> = t.iter().map(|r| r.kind).collect();
    assert_eq!(kinds.len(), 2, "{t:?}");
}

// §2.3, §3.3: two identical content-keyed units in one scope are one unit,
// and one record.
#[test]
fn one_record_for_a_repeated_content_keyed_unit() {
    let (recs, out) = snippet(
        "static_assert(true);\nstatic_assert(true);\nstatic_assert(1 == 1);\n",
        &RuleTable::empty(),
    );
    check(&recs, &out).unwrap();
    let sa: Vec<&Record> = recs.iter().filter(|r| r.name == "<static_assert>").collect();
    assert_eq!(sa.len(), 2, "{recs:#?}");
    for r in sa {
        let k = r.key.as_ref().unwrap();
        assert_eq!((k.kind, k.name.clone()), (KeyKind::StaticAssert, Name::Content));
    }
}

// §2.3 "as `digest` gives it under `rules`", digest §3.4: records under a
// rule table have the mapped ids and the mapped `Key`s: `old` relocated
// to `fresh` (the rule undoing `fresh` back to `old`, as §2.1 writes it)
// gives the post-image's entities the pre-image's keys.
#[test]
fn records_under_rules() {
    let pre = "namespace old { struct S { int m; }; void f(S); }\n";
    let post = "namespace fresh { struct S { int m; }; void f(S); }\n";
    let (pre_recs, _) = snippet(pre, &RuleTable::empty());
    let rule = Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: ident("fresh"),
        disc: None,
        to_parent: Parent::Root,
        to_name: ident("old"),
    };
    let rules = RuleTable::new(vec![rule]).unwrap();
    let (post_recs, out) = snippet(post, &rules);
    check(&post_recs, &out).unwrap();
    // Same ids and keys, other names.
    let strip = |v: &[Record]| -> Vec<(Digest, Option<Key>, KeyKind)> {
        v.iter().map(|r| (r.id, r.key.clone(), r.kind)).collect()
    };
    assert_eq!(strip(&pre_recs), strip(&post_recs));
    let ns = named(&post_recs, "fresh");
    assert_eq!(ns.key.as_ref().unwrap().name, ident("old"));
    assert_eq!(ns.id, named(&pre_recs, "old").id);
    assert_eq!(named(&post_recs, "fresh::f(fresh::S)").id, named(&pre_recs, "old::f(old::S)").id);
}

/// Pairs for which `records` is `Err`, with the reason: each is a finding
/// about the walk (a key that is not the same when computed again).
const ALLOWED_ERR: &[(&str, &str)] = &[];

/// What one side gave.
#[derive(Default)]
struct Tally {
    sides: u64,
    unparsed: Vec<String>,
    records: u64,
    with_key: u64,
    fallback: u64,
    indeterminate: u64,
    errs: Vec<String>,
    failures: Vec<String>,
}

fn sweep_side_with<R: Send>(
    dir: &Path,
    side: Side,
    t: &mut Tally,
    f: impl FnOnce(&Input<'_>) -> R + Send,
) -> Option<R> {
    let name = format!("{} {side:?}", common::rel(dir));
    let all = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                (n, std::fs::read_to_string(e.path()).unwrap_or_default())
            })
            .collect(),
        Err(e) => {
            t.failures.push(format!("{name}: {e}"));
            return None;
        }
    };
    let (list, head) = match common::side_files(&all, side) {
        Ok(x) => x,
        Err(e) => {
            t.failures.push(format!("{name}: {e}"));
            return None;
        }
    };
    // The main file first.
    let mut files: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let main = files.iter().position(|(n, _)| *n == "main.cpp").unwrap();
    files.swap(0, main);
    let args: Vec<&str> = head.args.iter().map(String::as_str).collect();
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: &head.std,
    };
    let extra = slip_kiln::snippet::Extra { args: &args, target: head.target.as_deref() };
    let got = slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let (ast, sm) = (p.ast, p.sm);
        common::on_walk_stack(|| {
            let path = |f: slip_kiln::kiln::source::FileId| {
                paths.get(f.index()).cloned().unwrap_or_default()
            };
            let input = Input { ast, sm, subject_path: &path };
            f(&input)
        })
    });
    t.sides += 1;
    match got {
        Ok(x) => Some(x),
        Err(e) if e.contains("panicked:") => {
            t.failures.push(format!("{name}: {e}"));
            None
        }
        Err(e) => {
            t.unparsed.push(format!("{name}: {}", e.lines().next().unwrap_or("")));
            None
        }
    }
}

fn sweep_side(dir: &Path, side: Side, t: &mut Tally) {
    let name = format!("{} {side:?}", common::rel(dir));
    let Some((recs, out)) = sweep_side_with(dir, side, t, |input| {
        let empty = RuleTable::empty();
        let recs = records(input, &empty, slip_digest::ReadOnly::EVERY_FILE);
        let out = slip_digest::digest(
            input,
            &Options {
                rules: &empty,
                read_only: slip_digest::ReadOnly::NO_FILE,
                detail: Detail::All,
                names: true,
                rule_use: false,
            },
        );
        (recs, out)
    }) else {
        return;
    };
    let recs = match recs {
        Ok(r) => r,
        Err(e) => {
            t.errs.push(format!("{name}: {e}"));
            return;
        }
    };
    if let Err(e) = check(&recs, &out) {
        t.failures.push(format!("{name}: {e}"));
    }
    t.records += recs.len() as u64;
    t.with_key += recs.iter().filter(|r| r.key.is_some()).count() as u64;
    t.fallback += recs.iter().filter(|r| r.key.is_none()).count() as u64;
    t.indeterminate += recs.iter().filter(|r| r.indeterminate).count() as u64;
}

#[test]
fn wp168_sweep_side_propagates_callback_panic() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/differ/_/dependent-decltype-own-lookup-set");
    let mut t = Tally::default();
    let got = sweep_side_with(&dir, Side::A, &mut t, |_| {
        assert_eq!(1, 2, "wp168 records sentinel");
    });
    assert!(got.is_none());
    assert!(t.unparsed.is_empty(), "a callback panic must not land in unparsed: {:?}", t.unparsed);
    assert_eq!(t.failures.len(), 1, "{:?}", t.failures);
    assert!(
        t.failures[0].contains("the callback panicked:")
            && t.failures[0].contains("wp168 records sentinel"),
        "{:?}",
        t.failures
    );
}

// §2.3 "Records" over every directory of the pair tests, both sides,
// pending ones included: `records` is `Ok` (but for the pairs of
// `ALLOWED_ERR`), its ids are exactly the unit keys of `digest` with
// detail `all`, and every record's `Key` has its id.
#[test]
fn records_of_every_pair_test() {
    let mut t = Tally::default();
    for root in ["differ", "equal", "demand", "indeterminate"] {
        for dir in common::dirs(root) {
            for side in [Side::A, Side::B] {
                sweep_side(&dir, side, &mut t);
            }
        }
    }
    eprintln!(
        "records: {} sides ({} did not parse), {} records, {} with a Key, {} fallback keys, {} indeterminate, {} Err",
        t.sides,
        t.unparsed.len(),
        t.records,
        t.with_key,
        t.fallback,
        t.indeterminate,
        t.errs.len()
    );
    for u in &t.unparsed {
        eprintln!("did not parse: {u}");
    }
    let mut unexpected = Vec::new();
    for e in &t.errs {
        eprintln!("Err: {e}");
        if !ALLOWED_ERR.iter().any(|(pair, _)| e.starts_with(pair)) {
            unexpected.push(e.clone());
        }
    }
    for (pair, why) in ALLOWED_ERR {
        assert!(
            t.errs.iter().any(|e| e.starts_with(pair)),
            "{pair} is allowed to be Err ({why}) but is not"
        );
    }
    assert!(t.sides > 0 && t.records > 0);
    assert!(t.failures.is_empty(), "{} failure(s):\n{}", t.failures.len(), t.failures.join("\n"));
    assert!(unexpected.is_empty(), "records is Err for:\n{}", unexpected.join("\n"));
}
