//! `spec/model.md` §8.5: the walk over a finished unit of the parse task
//! (`slip_digest::map`), on observers this test writes to a directory of
//! its own. Each test names the paragraph it covers ("The stack",
//! "Subject paths", "Counters", "The shard").
//!
//! The parse changes the process's current directory, so every test that
//! parses holds [`LOCK`] and uses absolute paths only.
//!
//! "The walk skipped on a unit with errors" is `map_unit`'s (`None` when
//! `unit.errors()` is not 0): `crates/slip_cli/src/worker.rs` tests it
//! through the worker, and `crates/slip_cli/tests/cli.rs` through the
//! executable. [`map_shard`] here composes the shard as the worker does,
//! so that the digest's part is tested on its own.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_digest::map::{cause_word, counters, map_unit, with_input};
use slip_digest::{Area, Cause, Detail, Options, Output, STACK_SIZE};
use slip_kiln::{ParseOptions, parse_observer_with};
use slip_model::digests::{Digests, GroupClass, SCHEME};
use slip_model::keys::RuleTable;
use slip_model::{
    Command, Counter, Digest, FileRef, Language, Observer, Origin, Phase, Shard, Status,
};

static LOCK: Mutex<()> = Mutex::new(());

/// A directory of the test's own (its real path), removed on drop, and
/// the lock held while it lives.
struct Fixture {
    root: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let base =
            std::env::temp_dir().join(format!("slip-digest-map-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let root = std::fs::canonicalize(&base).unwrap();
        Fixture { root, _lock: lock }
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    fn observer(&self, argv: &[&str]) -> Observer {
        Observer {
            id: Digest([5; 32]),
            command: Command {
                directory: self.root.to_str().unwrap().as_bytes().to_vec(),
                argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
                env: Vec::new(),
            },
            language: Language::Cxx,
            main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn bytes(p: &Path) -> Vec<u8> {
    p.to_str().unwrap().as_bytes().to_vec()
}

fn rules() -> RuleTable {
    RuleTable::empty()
}

/// The walk of `o`'s unit with detail `all` and names, through `subject`; `None`
/// when the unit had errors (the walk not run).
fn walk(
    o: &Observer,
    subject: &(dyn Fn(&[u8]) -> Vec<u8> + Sync),
) -> (Shard, Option<Result<Output, String>>) {
    let rules = rules();
    let options = Options { rules: &rules, detail: Detail::All, names: true };
    let (shard, walked) = parse_observer_with(o, Phase::Map, 0, ParseOptions::default(), |unit| {
        map_unit(unit, &options, subject)
    })
    .expect("parse_observer_with");
    (shard, walked.flatten())
}

/// The shard of phase `map` as §8.5 "The shard" makes it (the worker's
/// composition): the walk with detail `none` when the unit has no error;
/// an `ok` shard gets its `digests` and counters.
fn map_shard(o: &Observer) -> Result<Shard, String> {
    let rules = rules();
    let options = Options { rules: &rules, detail: Detail::None, names: false };
    let identity = |p: &[u8]| p.to_vec();
    let (mut shard, walked) =
        parse_observer_with(o, Phase::Map, 0, ParseOptions::default(), |unit| {
            map_unit(unit, &options, &identity)
        })?;
    if let Some(output) = walked.flatten().transpose()?
        && shard.outcome.status == Status::Ok
    {
        shard.counters.extend(counters(&output));
        shard.counters.sort_by(|a, b| a.name.cmp(&b.name));
        shard.digests = Some(output.digests);
    }
    Ok(shard)
}

fn declared_files(output: &Output) -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = output
        .digests
        .groups
        .iter()
        .filter(|g| g.class == GroupClass::Declared)
        .map(|g| g.file.clone())
        .collect();
    v.dedup();
    v
}

/// The counter grammar of `spec/model.md` §8.1:
/// `[a-z0-9_]+(\.[a-z0-9_]+)*`.
fn is_counter_name(name: &str) -> bool {
    name.split('.')
        .all(|w| !w.is_empty() && w.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_')))
}

// §8.5 "Subject paths": a buffer that is an input has `subject(real
// path)` (the identity here); every other buffer the front end made is
// named `<…>` and has that name; the declared groups are the inputs that
// hold declarations.
#[test]
fn s8_5_subject_paths_of_inputs_and_predefines() {
    let f = Fixture::new("subject");
    let header = f.write("inc/h.h", "#pragma once\nint helper(int);\nstruct S { int m; };\n");
    let main = f.write(
        "main.cpp",
        "#include \"inc/h.h\"\n#define CAT(a, b) a##b\nint CAT(wan, ted)(int x) { return helper(x) + M; }\n",
    );
    let o = f.observer(&["c++", "-std=c++17", "-DM=1", "main.cpp"]);
    let identity = |p: &[u8]| p.to_vec();
    let (shard, seen) = parse_observer_with(&o, Phase::Map, 0, ParseOptions::default(), |unit| {
        with_input(unit, &identity, |input| {
            let sm = input.sm;
            sm.file_ids()
                .map(|b| {
                    let real = unit.real_path(b).map(<[u8]>::to_vec);
                    (sm.file_name(b).to_owned(), real, (input.subject_path)(b))
                })
                .collect::<Vec<_>>()
        })
    })
    .unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    let buffers = seen.unwrap().expect("every buffer has a subject path");
    let mut virtuals = 0;
    for (name, real, subject) in &buffers {
        if let Some(real) = real {
            assert_eq!(subject, real, "{name}: an input is its real path");
        } else {
            assert!(name.starts_with('<'), "{name}: a buffer that is no input is `<…>`");
            assert_eq!(subject, name.as_bytes(), "{name}: has its name");
            virtuals += 1;
        }
    }
    assert!(virtuals >= 1, "the predefines are a buffer: {buffers:?}");

    let (_, walked) = walk(&o, &identity);
    let output = walked.unwrap().unwrap();
    assert_eq!(declared_files(&output), vec![bytes(&header), bytes(&main)]);
}

// §8.5 "Subject paths": `subject` maps the real path of an input; the
// groups carry what it returns (here a root mapped to `/subject`), and
// the digests are those of the identity with the files renamed.
#[test]
fn s8_5_subject_maps_a_root() {
    let f = Fixture::new("root");
    f.write("lib/h.h", "namespace n { int g(); }\n");
    f.write("main.cpp", "#include \"lib/h.h\"\nint n::g() { return 1; }\nint k = n::g();\n");
    let o = f.observer(&["c++", "main.cpp"]);
    let root = bytes(&f.root);
    let mapped = |p: &[u8]| -> Vec<u8> {
        match p.strip_prefix(root.as_slice()) {
            Some(rest) => [b"/subject".as_slice(), rest].concat(),
            None => p.to_vec(),
        }
    };
    let (_, walked) = walk(&o, &mapped);
    let output = walked.unwrap().unwrap();
    assert_eq!(
        declared_files(&output),
        vec![b"/subject/lib/h.h".to_vec(), b"/subject/main.cpp".to_vec()]
    );
    let identity = |p: &[u8]| p.to_vec();
    let (_, plain) = walk(&o, &identity);
    let plain = plain.unwrap().unwrap();
    let units = |o: &Output| -> Vec<_> {
        o.digests
            .groups
            .iter()
            .map(|g| (g.class, g.count, g.indeterminate, g.units.clone()))
            .collect()
    };
    assert_eq!(units(&output), units(&plain), "only the files differ");
}

// §8.5 "The stack": a command whose `-fkiln-stack-size=` leaves less than
// `STACK_SIZE` on the front end's thread is `Err`, and `f` is not called;
// Kiln's default (1 GiB) is enough.
#[test]
fn s8_5_too_little_stack_is_err() {
    let f = Fixture::new("stack");
    f.write("main.cpp", "int x;\n");
    let small = f.observer(&["c++", "-fkiln-stack-size=16M", "main.cpp"]);
    let identity = |p: &[u8]| p.to_vec();
    let (shard, seen) =
        parse_observer_with(&small, Phase::Map, 0, ParseOptions::default(), |unit| {
            let left = unit.stack_remaining();
            (left, with_input(unit, &identity, |_| -> u8 { panic!("f is not called") }))
        })
        .unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    let (left, result) = seen.unwrap();
    let left = left.expect("the front end's thread has a guard");
    assert!(left < 16 << 20, "{left}");
    let e = result.unwrap_err();
    let prefix = "the front end's thread has ";
    let suffix = format!(" bytes of stack left; the digest needs {STACK_SIZE}");
    assert!(e.starts_with(prefix) && e.ends_with(&suffix), "{e}");
    let n: usize = e[prefix.len()..e.len() - suffix.len()].parse().expect("a number");
    assert!(n < 16 << 20 && n > 0, "{e}");
    // Through the map shard: the same error (the figure is that of the
    // frame that measures it).
    let e = map_shard(&small).unwrap_err();
    assert!(e.starts_with(prefix) && e.ends_with(&suffix), "{e}");
    // A stack between half of `STACK_SIZE` and `STACK_SIZE` is too small
    // as well (review M02).
    let e = map_shard(&f.observer(&["c++", "-fkiln-stack-size=48M", "main.cpp"])).unwrap_err();
    assert!(e.starts_with(prefix) && e.ends_with(&suffix), "{e}");

    let (_, walked) = walk(&f.observer(&["c++", "main.cpp"]), &identity);
    assert!(walked.unwrap().is_ok());
}

// §8.5 "The shard" and §1 rule 2: two runs give the same shard bytes; the
// shard is valid, `ok`, and has `digests` and the walk's counters.
#[test]
fn s8_5_same_shard_bytes_from_two_runs() {
    let f = Fixture::new("twice");
    f.write("h.h", "template <class T> struct Box { T v; T get() const { return v; } };\n");
    f.write(
        "main.cpp",
        "#include \"h.h\"\nint use(Box<int> b) { return b.get(); }\nconst char *d = __DATE__;\n",
    );
    let o = f.observer(&["c++", "-std=c++20", "main.cpp"]);
    let first = map_shard(&o).unwrap();
    let second = map_shard(&o).unwrap();
    first.validate().expect("a valid shard");
    assert_eq!(first.outcome.status, Status::Ok, "{:?}", first.outcome);
    assert!(first.digests.is_some());
    let names: Vec<&str> = first.counters.iter().map(|c| c.name.as_str()).collect();
    for n in ["digest.bytes", "digest.groups", "digest.units", "inputs.files", "pp.tokens"] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    assert_eq!(first.to_bytes(), second.to_bytes());
}

// §8.5 "Counters": `output.counters` kept, one `digest.cause.<cause>.<word>`
// per key whose cause has a text, two texts with one word added up, the
// other causes not counted there; sorted by name.
#[test]
fn s8_5_cause_counters() {
    let k = |n: u8| Digest([n; 32]);
    let output = Output {
        digests: Digests { scheme: SCHEME, tu: Digest([0; 32]), groups: Vec::new() },
        counters: vec![
            Counter { name: "digest.bytes".into(), value: 9 },
            Counter { name: "digest.indeterminate".into(), value: 6 },
            Counter { name: "digest.indeterminate.unaccounted".into(), value: 3 },
        ],
        causes: vec![
            (k(1), Cause::Unaccounted("walk/depth")),
            (k(2), Cause::Unaccounted("Walk/Depth")),
            (k(3), Cause::Unaccounted("expr/aux")),
            (k(4), Cause::Unsupported("ExprKind/CXXReflectExpr")),
            (k(5), Cause::Pending(Area::Stmt)),
            (k(6), Cause::Collision),
        ],
        names: Vec::new(),
    };
    let got: Vec<(String, u64)> =
        counters(&output).into_iter().map(|c| (c.name, c.value)).collect();
    let want: Vec<(String, u64)> = [
        ("digest.bytes", 9),
        ("digest.cause.unaccounted.expr.aux", 1),
        ("digest.cause.unaccounted.walk.depth", 2),
        ("digest.cause.unsupported.exprkind.cxxreflectexpr", 1),
        ("digest.indeterminate", 6),
        ("digest.indeterminate.unaccounted", 3),
    ]
    .map(|(n, v)| (n.to_owned(), v))
    .to_vec();
    assert_eq!(got, want);
}

// §8.5 "Counters": the word of a cause's text.
#[test]
fn s8_5_cause_word() {
    for (text, word) in [
        ("ExprKind/CXXReflectExpr", "exprkind.cxxreflectexpr"),
        ("walk/depth", "walk.depth"),
        ("key/local-ref", "key.local_ref"),
        ("a b", "a_b"),
        ("", "_"),
        ("/", "_._"),
        ("a//b", "a._.b"),
        ("/a/", "_.a._"),
        // A `.` of the text is a byte like any other: only `/` separates.
        ("a.b", "a_b"),
        ("x..y", "x__y"),
        ("Dé", "d__"),
        ("snake_case_9", "snake_case_9"),
    ] {
        assert_eq!(cause_word(text), word, "{text:?}");
    }
    // Any text: a valid counter word.
    let every: String = (0u8..128).map(char::from).chain(['é', '→']).collect();
    for text in [every.as_str(), "..", "a/", "/a"] {
        let name = format!("digest.cause.unsupported.{}", cause_word(text));
        assert!(is_counter_name(&name), "{text:?}: {name}");
    }
}

/// Every string literal `"…"` right after `Unsupported(` or
/// `Unaccounted(` in `src/` (the texts written in place; the others come
/// from the tables, which the pair tests reach).
fn literal_texts() -> Vec<String> {
    let mut out = Vec::new();
    let src = common::manifest().join("src");
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(&src).unwrap().flatten().map(|e| e.path()).collect();
    files.sort();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for open in ["Unsupported(\"", "Unaccounted(\""] {
            let mut rest = text.as_str();
            while let Some(at) = rest.find(open) {
                rest = &rest[at + open.len()..];
                if let Some(end) = rest.find('"') {
                    out.push(rest[..end].to_owned());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

// §8.5 "Counters" and §8.1: every cause text of the crate makes a counter
// name of the shard's grammar. The texts are those written in place in
// `src/`, and every text the walk gives on the sources of the pair tests
// (which reach the tables' rows); for these, `counters` counts each key
// under its cause's word.
#[test]
fn s8_5_cause_word_on_every_cause_text() {
    let literals = literal_texts();
    assert!(literals.iter().any(|t| t == "walk/depth"), "{literals:?}");
    for text in &literals {
        let name = format!("digest.cause.unaccounted.{}", cause_word(text));
        assert!(is_counter_name(&name), "{text:?}: {name}");
    }

    let mut texts: BTreeMap<String, u64> = BTreeMap::new();
    for root in ["differ", "equal", "demand", "indeterminate"] {
        for dir in common::dirs(root) {
            if common::is_pending(&dir) {
                continue;
            }
            for side in [common::Side::A, common::Side::B] {
                let Ok(d) = common::digest_side(&dir, side) else { continue };
                let mut want: BTreeMap<String, u64> = BTreeMap::new();
                for (_, cause) in &d.output.causes {
                    if let Cause::Unsupported(t) | Cause::Unaccounted(t) = cause {
                        *texts.entry(format!("{cause}")).or_default() += 1;
                        let name = format!("digest.cause.{}.{}", cause.name(), cause_word(t));
                        *want.entry(name).or_default() += 1;
                    }
                }
                let got: BTreeMap<String, u64> = counters(&d.output)
                    .into_iter()
                    .filter(|c| c.name.starts_with("digest.cause."))
                    .map(|c| (c.name, c.value))
                    .collect();
                assert_eq!(got, want, "{} {side:?}", common::rel(&dir));
                let all = counters(&d.output);
                for c in &all {
                    assert!(is_counter_name(&c.name), "{}: {}", common::rel(&dir), c.name);
                }
                assert!(
                    all.windows(2).all(|w| w[0].name < w[1].name),
                    "{}: sorted and unique",
                    common::rel(&dir)
                );
            }
        }
    }
    assert!(!texts.is_empty(), "the pair tests reach causes with a text");
    eprintln!("cause texts reached by the pair tests: {texts:?}");
}

/// The `(class, file)` of the group of every unit named `name`.
fn groups_of(output: &Output, name: &str) -> Vec<(GroupClass, Vec<u8>)> {
    let keys: Vec<Digest> =
        output.names.iter().filter(|(_, n)| n == name).map(|(k, _)| *k).collect();
    output
        .digests
        .groups
        .iter()
        .flat_map(|g| {
            g.units
                .iter()
                .flatten()
                .filter(|u| keys.contains(&u.key))
                .map(|_| (g.class, g.file.clone()))
        })
        .collect()
}

/// The map shard of `main.cpp` (and `files`) is valid and `ok`, and the
/// walk with detail `all`.
fn valid_and_walked(f: &Fixture, files: &[(&str, &str)]) -> Output {
    for (rel, text) in files {
        f.write(rel, text);
    }
    let o = f.observer(&["c++", "-std=c++20", "main.cpp"]);
    let shard = map_shard(&o).expect("the map shard");
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    shard.validate().expect("a valid shard");
    let identity = |p: &[u8]| p.to_vec();
    let (_, walked) = walk(&o, &identity);
    walked.unwrap().unwrap()
}

// digest §11 "origin file" (WP-050 fix round, A1): Kiln declares the
// global allocation functions without a location before a source's first
// declaration of one; the declared unit's file is that of the first
// declaration that has one, so the shard is valid (it was `failed`, "a
// declared group has an empty file", on 3975 files of LLVM).
#[test]
fn s8_5_operator_new_redeclared_is_in_its_file() {
    let f = Fixture::new("opnew");
    let output = valid_and_walked(&f, &[("main.cpp", "void *operator new(decltype(sizeof 0));\n")]);
    let main = bytes(&f.root.join("main.cpp"));
    assert_eq!(
        groups_of(&output, "operator new(unsigned long)"),
        vec![(GroupClass::Declared, main)]
    );
    assert!(
        output.digests.groups.iter().all(|g| g.class != GroupClass::Declared || !g.file.is_empty()),
        "no declared group has the empty path"
    );
    // The other implicit allocation functions, and the implicit
    // `std::align_val_t`, are no unit: nothing refers to them (A2).
    assert_eq!(output.names.len(), 2, "{:?}", output.names);
}

// The same, the redeclaration written in a header: the unit is in the
// header's group.
#[test]
fn s8_5_operator_new_redeclared_in_a_header() {
    let f = Fixture::new("opnew-h");
    let output = valid_and_walked(
        &f,
        &[
            ("new.h", "#pragma once\nvoid operator delete(void *) noexcept;\n"),
            ("main.cpp", "#include \"new.h\"\nint *p = new int;\n"),
        ],
    );
    let header = bytes(&f.root.join("new.h"));
    assert_eq!(groups_of(&output, "operator delete(void *)"), vec![(GroupClass::Declared, header)]);
    // `operator new(unsigned long)`, which the new-expression refers to
    // and no source declares, is derived (A2), with the empty path.
    assert_eq!(
        groups_of(&output, "operator new(unsigned long)"),
        vec![(GroupClass::Derived, Vec::new())]
    );
}

// `<new>`'s shape (A6 iv): `std::align_val_t` declared in source after
// Kiln's implicit one, and an aligned allocation function.
#[test]
fn s8_5_align_val_t_redeclared() {
    let f = Fixture::new("align");
    let output = valid_and_walked(
        &f,
        &[(
            "main.cpp",
            "namespace std { enum class align_val_t : decltype(sizeof 0) {}; }\n\
             void *operator new(decltype(sizeof 0), std::align_val_t);\n",
        )],
    );
    let main = bytes(&f.root.join("main.cpp"));
    for name in ["std::align_val_t", "operator new(unsigned long, std::align_val_t)"] {
        assert_eq!(groups_of(&output, name), vec![(GroupClass::Declared, main.clone())], "{name}");
    }
    assert_eq!(groups_of(&output, "std"), vec![(GroupClass::Tu, Vec::new())]);
}

/// The units named `name` of the walk of `main.cpp` over `files`, as
/// `(class, file name, digest, flags)`.
fn units_named(
    f: &Fixture,
    files: &[(&str, &str)],
    name: &str,
) -> Vec<(GroupClass, String, Digest, u64)> {
    let output = valid_and_walked(f, files);
    let keys: Vec<Digest> =
        output.names.iter().filter(|(_, n)| n == name).map(|(k, _)| *k).collect();
    let mut out = Vec::new();
    for g in &output.digests.groups {
        for u in g.units.iter().flatten().filter(|u| keys.contains(&u.key)) {
            let file = String::from_utf8_lossy(&g.file).rsplit('/').next().unwrap_or("").to_owned();
            out.push((g.class, file, u.digest, u.flags));
        }
    }
    out
}

const TMPL_H: &str =
    "template <class T> struct A { friend void *operator new(decltype(sizeof 0)); };\n";
const NEW_H: &str = "void *operator new(decltype(sizeof 0));\n";

// Review F1 (WP-050 fix round): Kiln chains a friend of a class
// template's pattern to a later global `operator new` (its allocation
// path finds the friend by `lookup_local`). The unit is read from, and in
// the group of, the namespace-scope declaration: `new.h`, unflagged, with
// one digest whether or not `A<int>` is instantiated after `new.h`
// (digest §1 rule 5). It is not the digest of a unit without the
// template: there Kiln declares the implicit `operator new(size_t)`
// first and the chain reads it (A1), here `find_global_op` finds the
// friend and Kiln declares no implicit one.
#[test]
fn s8_5_pattern_friend_of_operator_new() {
    let mut seen = Vec::new();
    for (i, main) in [
        "#include \"tmpl.h\"\n#include \"new.h\"\n",
        "#include \"tmpl.h\"\n#include \"new.h\"\nA<int> a;\n",
    ]
    .into_iter()
    .enumerate()
    {
        let units = units_named(
            &Fixture::new(&format!("f1-{i}")),
            &[("tmpl.h", TMPL_H), ("new.h", NEW_H), ("main.cpp", main)],
            "operator new(unsigned long)",
        );
        assert_eq!(units.len(), 1, "placement {i}: {units:?}");
        assert_eq!(
            (units[0].0, units[0].1.as_str(), units[0].3),
            (GroupClass::Declared, "new.h", 0)
        );
        seen.push(units[0].2);
    }
    assert_eq!(seen[0], seen[1], "one digest whether or not A<int> follows");
}

// The third placement of F1, `A<int>` instantiated between the template
// and `new.h`: the instantiated friend is left unchained by Kiln (its
// allocation path chains `new.h`'s declaration to the pattern's friend,
// not to the instantiated one), so two entities have one key, both
// flagged `collision`. Kiln's to fix (`spec/landed/model/WP-050.md`); this
// pins what the walk does with it today: no `failed` shard, the declared
// unit still in `new.h`.
#[test]
fn s8_5_pattern_friend_instantiated_before_the_declaration() {
    let units = units_named(
        &Fixture::new("f1-between"),
        &[
            ("tmpl.h", TMPL_H),
            ("new.h", NEW_H),
            ("main.cpp", "#include \"tmpl.h\"\nA<int> a;\n#include \"new.h\"\n"),
        ],
        "operator new(unsigned long)",
    );
    let shape: Vec<(GroupClass, &str, u64)> =
        units.iter().map(|(c, f, _, fl)| (*c, f.as_str(), *fl)).collect();
    assert_eq!(shape, vec![(GroupClass::Declared, "new.h", 1), (GroupClass::Derived, "tmpl.h", 1)]);
}

// digest §11 origin file, review F3: Kiln's implicit `operator new` (made
// by the new-expression), then a redeclaration in `a.h`, then one in
// `main.cpp`: the unit is in the group of the earliest declaration that
// has a location, `a.h`, unflagged.
#[test]
fn s8_5_earliest_located_redeclaration() {
    let units = units_named(
        &Fixture::new("f3-order"),
        &[
            ("a.h", "void *operator new(decltype(sizeof 0));\n"),
            (
                "main.cpp",
                "int *p = new int;\n#include \"a.h\"\nvoid *operator new(decltype(sizeof 0));\n",
            ),
        ],
        "operator new(unsigned long)",
    );
    let shape: Vec<(GroupClass, &str, u64)> =
        units.iter().map(|(c, f, _, fl)| (*c, f.as_str(), *fl)).collect();
    assert_eq!(shape, vec![(GroupClass::Declared, "a.h", 0)]);
}
