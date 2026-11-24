//! `spec/verify.md` §5.2 to §5.4 and §7.2: the verify task run in process
//! on trees the test writes. The map shard is made by `MapTask` and
//! published to a store in a temporary directory under the target
//! directory, then altered or removed as a case needs.
//!
//! A parse changes the process's current directory: every test that
//! parses holds [`LOCK`], and every path is absolute.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_cli::image::{Image, RootMap};
use slip_cli::verify_job::{Params, verify};
use slip_cli::worker::{MapTask, PhaseTask};
use slip_exec::store::{Access, Store};
use slip_exec::worker::{Job, Post, PostImage, Task};
use slip_model::declared::Declared;
use slip_model::digests::tu_digest;
use slip_model::formats::content_id;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::transform::Ownership;
use slip_model::verdict::{FindingKind, VerdictKind};
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard, Status};

static LOCK: Mutex<()> = Mutex::new(());

/// A subject tree `s/`, a post tree `post-tree/` (a longer name, so that a
/// path spelled in either differs in length) and a store, in a directory
/// of their own that is removed on drop; the lock is held while it lives.
struct Tree {
    base: PathBuf,
    subject: PathBuf,
    post: PathBuf,
    store: Store,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn bytes(p: &Path) -> Vec<u8> {
    p.to_str().unwrap().as_bytes().to_vec()
}

impl Tree {
    fn new(name: &str, files: &[(&str, &str)]) -> Tree {
        Tree::new_in(name, "s", "post-tree", files)
    }

    /// A tree whose subject is `<base>/<subject>` and post tree
    /// `<base>/<post>`.
    fn new_in(name: &str, subject: &str, post: &str, files: &[(&str, &str)]) -> Tree {
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp052-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let subject = base.join(subject);
        let post = base.join(post);
        std::fs::create_dir_all(&subject).unwrap();
        std::fs::create_dir_all(&post).unwrap();
        for (rel, text) in files {
            write(&subject.join(rel), text);
            write(&post.join(rel), text);
        }
        let store = Store::open(&base.join("store"), Access::Shared).unwrap();
        Tree { base, subject, post, store, _lock: lock }
    }

    /// The observer compiling `s/a.cpp` from `s/`, by absolute path.
    fn observer(&self) -> Observer {
        self.observer_of("a.cpp", &[])
    }

    fn observer_of(&self, main: &str, flags: &[&str]) -> Observer {
        let mut argv: Vec<Vec<u8>> = vec![b"c++".to_vec(), b"-std=c++20".to_vec()];
        argv.extend(flags.iter().map(|f| f.as_bytes().to_vec()));
        argv.push(bytes(&self.subject.join(main)));
        let command = Command { directory: bytes(&self.subject), argv, env: Vec::new() };
        Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            main: FileRef {
                path: bytes(&self.subject.join(main)),
                size: 0,
                content: Digest([0; 32]),
            },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }
    }

    fn job(&self, observer: &Observer, phase: Phase, round: u64) -> Job {
        Job {
            store: bytes(self.store.root()),
            plan: Digest([7; 32]),
            phase,
            round,
            observer: observer.clone(),
            transform: None,
            post: None,
        }
    }

    /// The map shard of `observer`, made by the map task and published.
    fn map(&self, observer: &Observer) -> Shard {
        let job = self.job(observer, Phase::Map, 0);
        let shard = MapTask::new().run(&job).unwrap();
        self.store
            .publish_shard(&job.plan, Phase::Map, 0, &observer.id, &shard.to_bytes())
            .unwrap();
        shard
    }

    /// The image mapping `s/` to `post-tree/`.
    fn image(&self) -> Image {
        Image::new(vec![RootMap { subject: bytes(&self.subject), post: bytes(&self.post) }])
            .unwrap()
    }

    /// The run's files (`spec/edits.md` §2.1): the subject owned, nothing
    /// read-only; or, with `readonly`, these files of it read-only.
    fn files(&self, readonly: &[&str]) -> Ownership {
        let readonly = readonly.iter().map(|r| bytes(&self.subject.join(r))).collect();
        Ownership { owned: vec![bytes(&self.subject)], readonly }
    }

    fn params(&self, declared: Declared) -> Params {
        // The content id of its container; a value no container can hold
        // (rules without files, which only tests make) has none.
        let declared_id = match declared.validate() {
            Ok(()) => content_id(&declared.to_bytes()).unwrap(),
            Err(_) => Digest([0; 32]),
        };
        Params { pre: 0, declared, declared_id, image: self.image(), limit: 64, locate: 16 }
    }

    /// The verify task's shard of `observer`, round 1, validated.
    fn verify_with(&self, observer: &Observer, params: &Params) -> Result<Shard, String> {
        let job = self.job(observer, Phase::Verify, 1);
        let shard = verify(&self.store, &job, params)?;
        shard.validate().unwrap();
        assert_eq!((shard.observer, shard.phase, shard.round), (observer.id, Phase::Verify, 1));
        Ok(shard)
    }

    fn verify(&self, observer: &Observer) -> Shard {
        self.verify_with(observer, &self.params(Declared::none())).unwrap()
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn counter(shard: &Shard, name: &str) -> Option<u64> {
    shard.counters.iter().find(|c| c.name == name).map(|c| c.value)
}

fn kind(shard: &Shard) -> VerdictKind {
    shard.verdict.as_ref().expect("a verdict").kind
}

fn reason(shard: &Shard) -> &str {
    &shard.verdict.as_ref().expect("a verdict").reason
}

impl Tree {
    fn path(&self, rel: &str) -> String {
        self.subject.join(rel).to_str().unwrap().to_owned()
    }

    /// The map shard as stored, and its content id.
    fn stored_map(&self, observer: &Observer) -> (Shard, Digest) {
        let bytes =
            self.store.read_shard(&Digest([7; 32]), Phase::Map, 0, &observer.id).unwrap().unwrap();
        (Shard::from_bytes(&bytes).unwrap(), content_id(&bytes).unwrap())
    }

    /// Replaces the stored map shard of `observer` by `shard` (an edit of
    /// the store, as P1 to P3 and R2 are made).
    fn put_map(&self, observer: &Observer, shard: &Shard) -> Digest {
        shard.validate().unwrap();
        let bytes = shard.to_bytes();
        let path = self.store.shard_path(&Digest([7; 32]), Phase::Map, 0, &observer.id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        content_id(&bytes).unwrap()
    }

    fn params_with(&self, limit: usize, locate: usize) -> Params {
        Params { limit, locate, ..self.params(Declared::none()) }
    }

    /// The observer compiling `s/a.cpp`, by absolute path, from
    /// `directory`, with `flags` verbatim.
    fn observer_in(&self, directory: &Path, flags: &[String]) -> Observer {
        std::fs::create_dir_all(directory).unwrap();
        let mut o = self.observer();
        let mut argv: Vec<Vec<u8>> = vec![b"c++".to_vec(), b"-std=c++20".to_vec()];
        argv.extend(flags.iter().map(|f| f.as_bytes().to_vec()));
        argv.push(bytes(&self.subject.join("a.cpp")));
        o.command = Command { directory: bytes(directory), argv, env: Vec::new() };
        o.id = o.command.observer_id();
        o
    }

    /// The truth of a case (the reviewer's check): the subject with the
    /// edit applied (the post tree copied over it), mapped with the
    /// observer's own command; whether its digests differ from `m`'s, the
    /// pre-image's. The subject is changed: call it last.
    fn truth_differs(&self, o: &Observer, m: &Shard) -> bool {
        fn copy(from: &Path, to: &Path) {
            for e in std::fs::read_dir(from).unwrap() {
                let e = e.unwrap();
                let t = to.join(e.file_name());
                if e.file_type().unwrap().is_dir() {
                    std::fs::create_dir_all(&t).unwrap();
                    copy(&e.path(), &t);
                } else {
                    std::fs::copy(e.path(), &t).unwrap();
                }
            }
        }
        copy(&self.post, &self.subject);
        let now = MapTask::new().run(&self.job(o, Phase::Map, 0)).unwrap();
        assert_eq!(now.outcome.status, Status::Ok, "{:?}", now.outcome);
        now.digests != m.digests
    }
}

fn arg(prefix: &str, p: &Path) -> String {
    format!("{prefix}{}", p.to_str().unwrap())
}

fn ident(s: &str) -> Name {
    Name::Identifier(s.as_bytes().to_vec())
}

/// The rule renaming the namespace `from_post` of the post-image to
/// `to_pre` of the pre-image.
fn rename_ns(from_post: &str, to_pre: &str) -> Rule {
    Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: ident(from_post),
        disc: None,
        to_parent: Parent::Root,
        to_name: ident(to_pre),
    }
}

/// The §7.2 counters every verdict shard has.
const VERIFY_COUNTERS: [&str; 12] = [
    "verify.derived.only_post",
    "verify.derived.only_pre",
    "verify.exemptions.used",
    "verify.findings",
    "verify.findings.changed",
    "verify.findings.derived_changed",
    "verify.findings.derived_indeterminate",
    "verify.findings.added",
    "verify.findings.indeterminate",
    "verify.findings.removed",
    "verify.groups.differing",
    "verify.reparsed",
];

/// §5.4: the shard of a verdict has every §7.2 counter, counters sorted,
/// and digests without unit lists.
fn check_post_shard(v: &Shard) {
    for name in VERIFY_COUNTERS {
        assert!(counter(v, name).is_some(), "{name} missing");
    }
    let d = v.digests.as_ref().expect("digests");
    assert!(d.groups.iter().all(|g| g.units.is_none()), "a unit list is stored");
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(counter(v, "verify.findings"), Some(verdict.total));
    let by_kind: u64 = VERIFY_COUNTERS
        .iter()
        .filter(|n| n.starts_with("verify.findings.") && !n.ends_with("findings"))
        .map(|n| counter(v, n).unwrap())
        .sum();
    if verdict.kind == VerdictKind::Unverifiable {
        assert_eq!(by_kind, 0);
    } else {
        assert_eq!(by_kind, verdict.total);
    }
}

const OVERLOAD_PRE: &str = "int g(int);\nint g(long);\nint f() { return g(1); }\n";
const OVERLOAD_POST: &str = "int g(int);\nint g(long);\nint f() { return g(1L); }\n";

// §5.2 step 3: identical images are verified at the sums with one parse;
// the verdict names the map shard and the rules; §5.4's shard.
#[test]
fn s5_2_verified_at_the_sums() {
    let tree = Tree::new(
        "sums",
        &[("a.cpp", "#include \"h.h\"\nint f() { return k; }\n"), ("h.h", "const int k = 1;\n")],
    );
    let o = tree.observer();
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Ok);
    let (_, pre_id) = tree.stored_map(&o);
    let v = tree.verify(&o);
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(verdict.kind, VerdictKind::Verified);
    assert_eq!(verdict.pre, pre_id);
    assert_eq!(verdict.rules, Declared::none().rules().id());
    assert_eq!((verdict.total, verdict.findings.len()), (0, 0));
    assert_eq!(counter(&v, "verify.reparsed"), Some(0));
    assert_eq!(counter(&v, "verify.groups.differing"), Some(0));
    check_post_shard(&v);
    // §5.4: inputs are real paths in the post-image; digests are those
    // of the map shard (subject paths).
    let inputs = v.inputs.as_ref().unwrap();
    assert!(inputs.iter().any(|f| f.path == bytes(&tree.post.join("h.h"))));
    assert!(!inputs.iter().any(|f| f.path.starts_with(&bytes(&tree.subject))));
    assert_eq!(v.digests, m.digests);
}

// §5.2 steps 4 to 6: a call that now resolves to another overload is a
// mismatch, found by the comparison, named from the pre-image; three
// parses (step 4 and step 6).
#[test]
fn s5_2_mismatch_on_a_changed_callee() {
    let tree = Tree::new("callee", &[("a.cpp", OVERLOAD_PRE)]);
    write(&tree.post.join("a.cpp"), OVERLOAD_POST);
    let o = tree.observer();
    tree.map(&o);
    let (_, pre_id) = tree.stored_map(&o);
    let v = tree.verify(&o);
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(verdict.kind, VerdictKind::Mismatch);
    assert_eq!(verdict.pre, pre_id);
    assert!(verdict.total >= 1);
    assert_eq!(counter(&v, "verify.reparsed"), Some(2));
    assert!(counter(&v, "verify.groups.differing").unwrap() >= 1);
    assert!(counter(&v, "verify.findings.changed").unwrap() >= 1);
    check_post_shard(&v);
    let f = verdict.findings.iter().find(|f| f.kind == FindingKind::Changed).unwrap();
    assert!(f.name.contains('f'), "{:?}", f.name);
    // WP-054: the finding is placed (the site is `tests/locate.rs`'s).
    assert!(f.location.is_some());
}

const NOTE_PRE: &str = "void take(const char *);\nnamespace ns { struct V {}; }\n\
     void f(ns::V) { take(__PRETTY_FUNCTION__); }\nint g() { take(__func__); return 1; }\n";

// WP-112 (verify §4.4, §8): a parameter type requalified changes the
// string `f` passes on and nothing `f` does: verified, with one note for
// `f`; `g`'s string is unchanged and gives none. The map shard carries
// the section with both units' entries.
#[test]
fn names_note_beside_verified() {
    let tree = Tree::new("note", &[("a.cpp", NOTE_PRE)]);
    write(&tree.post.join("a.cpp"), &NOTE_PRE.replace("void f(ns::V)", "void f(::ns::V)"));
    let o = tree.observer();
    let m = tree.map(&o);
    assert_eq!(m.names.as_ref().expect("names").units.len(), 2);
    assert!(m.names.as_ref().unwrap().notes.is_empty());
    let v = tree.verify(&o);
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(verdict.kind, VerdictKind::Verified);
    assert_eq!(verdict.total, 0);
    check_post_shard(&v);
    let notes = &v.names.as_ref().expect("names").notes;
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].name, "f(ns::V)");
}

// WP-112: a note beside a finding of the same key (the body changed
// too): a mismatch keeps its notes.
#[test]
fn names_note_beside_a_finding() {
    let tree = Tree::new("note-mismatch", &[("a.cpp", NOTE_PRE)]);
    let post = NOTE_PRE.replace(
        "void f(ns::V) { take(__PRETTY_FUNCTION__); }",
        "void f(::ns::V) { take(__PRETTY_FUNCTION__); take(0); }",
    );
    write(&tree.post.join("a.cpp"), &post);
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify(&o);
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(verdict.kind, VerdictKind::Mismatch);
    assert!(
        verdict.findings.iter().any(|f| f.kind == FindingKind::Changed && f.name == "f(ns::V)")
    );
    check_post_shard(&v);
    assert_eq!(v.names.as_ref().unwrap().notes.len(), 1);
}

// WP-112: `g` changed and the string it passes on did not: a finding,
// no note. (A test of its own: a `Tree` holds the lock while it lives.)
#[test]
fn names_no_note_without_a_string_change() {
    let same = Tree::new("note-none", &[("a.cpp", NOTE_PRE)]);
    write(&same.post.join("a.cpp"), &NOTE_PRE.replace("return 1;", "return 2;"));
    let o = same.observer();
    same.map(&o);
    let v = same.verify(&o);
    assert_eq!(v.verdict.as_ref().unwrap().kind, VerdictKind::Mismatch);
    assert!(v.names.as_ref().unwrap().notes.is_empty());
}

// WP-112 (`spec/model.md` §8.7): the notes are computed before step 4;
// when step 4 refuses (R1: the subject no longer parses) the verdict is
// unverifiable and the shard keeps its names units but no note.
#[test]
fn names_no_note_beside_unverifiable() {
    let tree = Tree::new("note-r1", &[("a.cpp", NOTE_PRE)]);
    let post =
        NOTE_PRE.replace("void f(ns::V)", "void f(::ns::V)").replace("return 1;", "return 2;");
    write(&tree.post.join("a.cpp"), &post);
    let o = tree.observer();
    tree.map(&o);
    write(&tree.subject.join("a.cpp"), "int a = ;\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert!(reason(&v).starts_with("pre-image: no longer parses"), "{}", reason(&v));
    check_post_shard(&v);
    let names = v.names.as_ref().expect("names");
    assert_eq!(names.units.len(), 2);
    assert!(names.notes.is_empty(), "{:?}", names.notes);
}

// §5.2 step 6 and §6.1: with `locate` 0 no finding is named and the
// post-image is parsed once more only for step 4; with `locate` 1 only
// the first finding is.
#[test]
fn s6_1_locate() {
    let pre = "int a = 1;\nint b = 1;\nint c = 1;\n";
    let post = "int a = 2;\nint b = 2;\nint c = 2;\n";
    let tree = Tree::new("locate", &[("a.cpp", pre)]);
    write(&tree.post.join("a.cpp"), post);
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify_with(&o, &tree.params_with(64, 0)).unwrap();
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!((verdict.kind, verdict.total), (VerdictKind::Mismatch, 3));
    assert_eq!(counter(&v, "verify.reparsed"), Some(1));
    assert!(verdict.findings.iter().all(|f| f.name.is_empty()));
    let v = tree.verify_with(&o, &tree.params_with(64, 1)).unwrap();
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(counter(&v, "verify.reparsed"), Some(2));
    let named: Vec<&str> = verdict.findings.iter().map(|f| f.name.as_str()).collect();
    assert!(!named[0].is_empty() && named[1..].iter().all(|n| n.is_empty()), "{named:?}");
    let v = tree.verify_with(&o, &tree.params_with(64, 16)).unwrap();
    let mut named: Vec<String> = v.verdict.unwrap().findings.into_iter().map(|f| f.name).collect();
    named.sort();
    assert_eq!(named, ["a", "b", "c"]);
}

// §6.1: a finding of a unit the post-image no longer has is named from
// the pre-image.
#[test]
fn s6_1_removed_named_from_the_pre_image() {
    let tree = Tree::new("removed", &[("a.cpp", "int gone = 1;\nint kept = 1;\n")]);
    write(&tree.post.join("a.cpp"), "int kept = 1;\n");
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify(&o);
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(verdict.kind, VerdictKind::Mismatch);
    // `kept` is unchanged; any other finding is not a removal.
    let removed: Vec<&str> = verdict
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::Removed)
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(removed, ["gone"], "{:?}", verdict.findings);
    assert_eq!(counter(&v, "verify.findings.removed"), Some(1));
}

// §6: `limit` keeps the first findings; `total` and the counters count
// them all.
#[test]
fn s6_limit() {
    let pre = "int a = 1;\nint b = 1;\nint c = 1;\nint d = 1;\nint e = 1;\n";
    let tree = Tree::new("limit", &[("a.cpp", pre)]);
    write(&tree.post.join("a.cpp"), &pre.replace('1', "2"));
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify_with(&o, &tree.params_with(2, 16)).unwrap();
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(
        (verdict.kind, verdict.total, verdict.findings.len()),
        (VerdictKind::Mismatch, 5, 2)
    );
    assert_eq!(counter(&v, "verify.findings"), Some(5));
    assert_eq!(counter(&v, "verify.findings.changed"), Some(5));
    check_post_shard(&v);
    let v = tree.verify_with(&o, &tree.params_with(0, 16)).unwrap();
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(
        (verdict.kind, verdict.total, verdict.findings.len()),
        (VerdictKind::Mismatch, 5, 0)
    );
}

// §2.2, §7.2: a declared changed key exempts its finding, and is counted
// in `verify.exemptions.used`.
#[test]
fn s7_2_exemptions_used() {
    let tree = Tree::new("exempt", &[("a.cpp", "int a = 1;\nint b = 1;\n")]);
    write(&tree.post.join("a.cpp"), "int a = 2;\nint b = 1;\n");
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify(&o);
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!((verdict.kind, verdict.total), (VerdictKind::Mismatch, 1));
    let key = verdict.findings[0].key.unwrap();
    let declared = Declared::new(Vec::new(), vec![key], Vec::new(), Vec::new()).unwrap();
    let v = tree.verify_with(&o, &tree.params(declared)).unwrap();
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(verdict.kind, VerdictKind::Verified);
    assert_eq!(verdict.used, [key]);
    assert_eq!(counter(&v, "verify.exemptions.used"), Some(1));
    assert_eq!(counter(&v, "verify.reparsed"), Some(1));
    check_post_shard(&v);
}

// §5.2 step 1: a post-image that does not parse gives the parse task's
// shard: no digests, no verdict (`not_parsed`), whatever the pre shard.
#[test]
fn s5_2_post_image_not_parsed() {
    let tree = Tree::new("noparse", &[("a.cpp", "int a = 1;\n")]);
    write(&tree.post.join("a.cpp"), "int a = ;\n");
    let o = tree.observer();
    // No map shard yet: `not_parsed`, not P1.
    let v = tree.verify(&o);
    assert_eq!(v.outcome.status, Status::Diagnosed);
    assert!(v.verdict.is_none() && v.digests.is_none());
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(v.outcome.status, Status::Diagnosed);
    assert!(v.verdict.is_none() && v.digests.is_none());
    assert!(v.outcome.message.contains("post-tree/a.cpp"), "{}", v.outcome.message);
}

// §5.3 P1.
#[test]
fn p1_no_map_shard() {
    let tree = Tree::new("p1", &[("a.cpp", "int a = 1;\n")]);
    let o = tree.observer();
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert_eq!(reason(&v), "pre-image: no map shard");
    assert_eq!(v.verdict.as_ref().unwrap().pre, Digest([0; 32]));
    check_post_shard(&v);
}

// §5.3 P2: `diagnosed` from a real parse of a broken subject; `rejected`
// and `failed` by editing the store.
#[test]
fn p2_map_shard_not_ok() {
    let tree = Tree::new("p2", &[("a.cpp", "int a = ;\n")]);
    write(&tree.post.join("a.cpp"), "int a = 1;\n");
    let o = tree.observer();
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Diagnosed);
    let (_, id) = tree.stored_map(&o);
    let v = tree.verify(&o);
    assert_eq!(reason(&v), "pre-image: the map shard is diagnosed");
    assert_eq!(v.verdict.as_ref().unwrap().pre, id);
    check_post_shard(&v);

    let mut rejected = m.clone();
    rejected.outcome.status = Status::Rejected;
    let id = tree.put_map(&o, &rejected);
    let v = tree.verify(&o);
    assert_eq!(reason(&v), "pre-image: the map shard is rejected");
    assert_eq!(v.verdict.as_ref().unwrap().pre, id);

    let mut failed = m;
    failed.outcome.status = Status::Failed;
    failed.inputs = None;
    failed.negatives = None;
    failed.counters.clear();
    tree.put_map(&o, &failed);
    let v = tree.verify(&o);
    assert_eq!(reason(&v), "pre-image: the map shard is failed");
}

// §5.3 P3, by editing the store.
#[test]
fn p3_map_shard_without_digests() {
    let tree = Tree::new("p3", &[("a.cpp", "int a = 1;\n")]);
    let o = tree.observer();
    let mut m = tree.map(&o);
    m.digests = None;
    m.names = None;
    let id = tree.put_map(&o, &m);
    let v = tree.verify(&o);
    assert_eq!(reason(&v), "pre-image: the map shard has no digests");
    assert_eq!(v.verdict.as_ref().unwrap().pre, id);
    check_post_shard(&v);
}

// Step 0: a stored map shard that is not valid, or is another
// observer's, fails the job; no verdict is made from it. (This is also
// why guard C cannot be reached through the store: a `tu` that is not
// the digest of its groups is refused by the shard's decoder.)
#[test]
fn s5_2_pre_shard_altered_fails_the_job() {
    let tree = Tree::new("altered", &[("a.cpp", "int a = 1;\n")]);
    let o = tree.observer();
    let m = tree.map(&o);
    let path = tree.store.shard_path(&Digest([7; 32]), Phase::Map, 0, &o.id);
    let mut bad = m.clone();
    bad.digests.as_mut().unwrap().tu = Digest([9; 32]);
    assert!(bad.validate().is_err());
    let mut bytes = m.to_bytes();
    let last = bytes.len() - 40;
    bytes[last] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    let e = tree.verify_with(&o, &tree.params(Declared::none())).unwrap_err();
    assert!(e.contains("is not a valid shard"), "{e}");

    let other = tree.observer_of("a.cpp", &["-DX"]);
    let mut theirs = tree.map(&other);
    theirs.round = 0;
    std::fs::write(&path, theirs.to_bytes()).unwrap();
    let e = tree.verify_with(&o, &tree.params(Declared::none())).unwrap_err();
    assert!(e.contains(&format!("is for observer {}", other.id)), "{e}");

    // Ours, but of another round or phase, at the path of round 0.
    let mut round = m.clone();
    round.round = 3;
    std::fs::write(&path, round.to_bytes()).unwrap();
    let e = tree.verify_with(&o, &tree.params(Declared::none())).unwrap_err();
    assert!(e.contains(&format!("is for observer {} Map round 3", o.id)), "{e}");
    let mut phase = m;
    phase.phase = Phase::Verify;
    phase.negatives = None;
    std::fs::write(&path, phase.to_bytes()).unwrap();
    let e = tree.verify_with(&o, &tree.params(Declared::none())).unwrap_err();
    assert!(e.contains(&format!("is for observer {} Verify round 0", o.id)), "{e}");
}

// §5.3 G1: the post-image read a file of the subject by an absolute path.
#[test]
fn g1_absolute_include() {
    let tree = Tree::new("g1abs", &[("a.cpp", "#include \"h.h\"\n"), ("h.h", "int a = 1;\n")]);
    let h = tree.path("h.h");
    write(&tree.post.join("a.cpp"), &format!("#include \"{h}\"\n"));
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(reason(&v), format!("post-image: read {h}, a file of the subject"));
    check_post_shard(&v);
}

/// §9.1: an `ok` shard of a job under a rule table that is not empty has
/// the job's `binding` and a `rule_use` of the table's `rules_id`.
fn check_sections_under_rules(v: &Shard, params: &Params) {
    assert_eq!(v.outcome.status, Status::Ok);
    let want = slip_model::Binding { overlay: None, declared: params.declared_id, pre: params.pre };
    assert_eq!(v.binding, Some(want));
    assert_eq!(v.rule_use.as_ref().map(|u| u.rules), Some(params.declared.rules_id()));
}

// §5.3 G1 and §9.1 (WP-121, review F2): an image guard's `unverifiable`
// shard under a rule table that is not empty is an `ok` shard: it has
// `binding` and `rule_use`.
#[test]
fn g1_under_rules_keeps_the_sections() {
    let tree = Tree::new("g1rules", &[("a.cpp", "#include \"h.h\"\n"), ("h.h", "int a = 1;\n")]);
    let h = tree.path("h.h");
    write(&tree.post.join("a.cpp"), &format!("#include \"{h}\"\n"));
    let o = tree.observer();
    tree.map(&o);
    let params = tree.params(declared_of(&tree, rename_ns("fresh", "old"), &[]));
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(reason(&v), format!("post-image: read {h}, a file of the subject"));
    check_sections_under_rules(&v, &params);
}

// §5.3 G1: through an absolute include directory into the subject that
// does not spell the subject root, in a word the rewrite does not resolve
// (`-Wp,`), so the post command keeps it.
#[test]
fn g1_absolute_include_directory() {
    let tree = Tree::new("g1absi", &[("a.cpp", "#include \"h.h\"\n"), ("inc/h.h", "int a = 1;\n")]);
    let dir = format!("-Wp,-I{}/./s/inc", tree.base.to_str().unwrap());
    let o = tree.observer_of("a.cpp", &[&dir]);
    tree.map(&o);
    let v = tree.verify(&o);
    let h = tree.path("inc/h.h");
    assert_eq!(reason(&v), format!("post-image: read {h}, a file of the subject"));
}

// §5.3 G1: through an include directory relative to the post root, in a
// word the rewrite does not resolve (`-Wp,`).
#[test]
fn g1_relative_include_directory() {
    let tree = Tree::new("g1rel", &[("a.cpp", "#include \"h.h\"\n"), ("inc/h.h", "int a = 1;\n")]);
    let o = tree.observer_of("a.cpp", &["-Wp,-I../s/inc"]);
    tree.map(&o);
    let v = tree.verify(&o);
    let h = tree.path("inc/h.h");
    assert_eq!(reason(&v), format!("post-image: read {h}, a file of the subject"));
}

// §5.3 G1: through a symbolic link in the post tree to the subject;
// inputs are real paths.
#[test]
fn g1_symlink() {
    let tree =
        Tree::new("g1link", &[("a.cpp", "#include \"inc/h.h\"\n"), ("inc/h.h", "int a = 1;\n")]);
    std::fs::remove_dir_all(tree.post.join("inc")).unwrap();
    std::os::unix::fs::symlink(tree.subject.join("inc"), tree.post.join("inc")).unwrap();
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify(&o);
    let h = tree.path("inc/h.h");
    assert_eq!(reason(&v), format!("post-image: read {h}, a file of the subject"));
}

// §5.3 G1 with the identity image: no subject root, it holds; the
// subject is compared with itself.
#[test]
fn g1_identity_image() {
    let tree = Tree::new("g1id", &[("a.cpp", "#include \"h.h\"\n"), ("h.h", "int a = 1;\n")]);
    let o = tree.observer();
    tree.map(&o);
    let params = Params { image: Image::identity(), ..tree.params(Declared::none()) };
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(kind(&v), VerdictKind::Verified);
    assert!(v.inputs.as_ref().unwrap().iter().any(|f| f.path == bytes(&tree.subject.join("h.h"))));
}

// §5.3 R1: the subject no longer parses at step 4.
#[test]
fn r1_pre_image_no_longer_parses() {
    let tree = Tree::new("r1", &[("a.cpp", "int a = 1;\n")]);
    write(&tree.post.join("a.cpp"), "int a = 2;\n");
    let o = tree.observer();
    tree.map(&o);
    write(&tree.subject.join("a.cpp"), "int a = ;\n");
    let broken = MapTask::new().run(&tree.job(&o, Phase::Map, 5)).unwrap();
    assert_eq!(broken.outcome.status, Status::Diagnosed);
    let v = tree.verify(&o);
    assert_eq!(
        reason(&v),
        format!("pre-image: no longer parses (diagnosed): {}", broken.outcome.message)
    );
    assert_eq!(counter(&v, "verify.reparsed"), Some(1));
    check_post_shard(&v);
}

// §5.3 G2: a header of the subject edited after the map phase, with the
// identity image ("the tree now" against "the tree as planned") and with
// a post tree.
#[test]
fn g2_header_edited() {
    let tree =
        Tree::new("g2", &[("a.cpp", "#include \"h.h\"\nint b = a;\n"), ("h.h", "int a = 1;\n")]);
    let o = tree.observer();
    tree.map(&o);
    write(&tree.subject.join("h.h"), "int a = 2;\n");
    let expect = format!("pre-image: {} changed since the map phase", tree.path("h.h"));
    let params = Params { image: Image::identity(), ..tree.params(Declared::none()) };
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!((kind(&v), reason(&v)), (VerdictKind::Unverifiable, expect.as_str()));
    check_post_shard(&v);
    write(&tree.post.join("h.h"), "int a = 3;\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), expect);
}

// §5.3 G2: a file in one list only, after every file in both (path
// order): a header the subject's `__has_include` finds only after the
// map phase, and one it found then that is gone.
#[test]
fn g2_header_added_and_removed() {
    let main = "#if __has_include(\"zz.h\")\n#include \"zz.h\"\n#endif\nint a = 1;\n";
    for added in [true, false] {
        let tree = Tree::new("g2zz", &[("a.cpp", main)]);
        write(&tree.post.join("a.cpp"), "int a = 2;\n");
        if !added {
            write(&tree.subject.join("zz.h"), "int z = 1;\n");
        }
        let o = tree.observer();
        tree.map(&o);
        if added {
            write(&tree.subject.join("zz.h"), "int z = 1;\n");
        } else {
            std::fs::remove_file(tree.subject.join("zz.h")).unwrap();
        }
        let v = tree.verify(&o);
        let expect = format!("pre-image: {} changed since the map phase", tree.path("zz.h"));
        assert_eq!(reason(&v), expect, "added {added}");
    }
}

// §5.3 G2's note: a subject edited after the map phase without a change
// of digest is verified at the sums; G2 is not reached.
#[test]
fn g2_not_reached_at_the_sums() {
    let tree = Tree::new("g2sums", &[("a.cpp", "#include \"h.h\"\n"), ("h.h", "int a = 1;\n")]);
    let o = tree.observer();
    tree.map(&o);
    write(&tree.subject.join("h.h"), "// a comment\nint a = 1;\n");
    let params = Params { image: Image::identity(), ..tree.params(Declared::none()) };
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(kind(&v), VerdictKind::Verified);
}

// §5.3 R2, by editing the store: a stored group sum (and so `tu`) that a
// parse of the same inputs does not give. A front end that is a function
// of its inputs never gives this from real parses alone.
#[test]
fn r2_digests_not_those_of_the_map_shard() {
    let tree = Tree::new("r2", &[("a.cpp", "int a = 1;\n")]);
    let o = tree.observer();
    let mut m = tree.map(&o);
    let d = m.digests.as_mut().unwrap();
    d.groups[0].digest = Digest([5; 32]);
    d.tu = tu_digest(&d.groups);
    tree.put_map(&o, &m);
    let v = tree.verify(&o);
    assert_eq!(reason(&v), "pre-image: its digests are not those of the map shard");
    assert_eq!(counter(&v, "verify.reparsed"), Some(1));
    check_post_shard(&v);
}

// §5.3 G3, from real parses: a header of the post tree replaced, again
// and again, by a writer outside the job. Each attempt is a mismatch
// (the same post-image at both parses) or G3 (another one at step 6);
// never `verified`. G3 has to be seen within the attempts. In the second
// case the two contents differ in a space only: the `tu` digest is the
// same and the `inputs` are not, which G3 refuses too (WP-052's reading,
// spec/landed/verify/WP-052.md).
#[test]
fn g3_post_image_changed_during_the_job() {
    for (case, one, two) in
        [("tu", "int a = 1;\n", "int a = 2;\n"), ("inputs", "int a = 1;\n", "int a = 1; \n")]
    {
        g3_case(case, one, two, false);
    }
}

// §5.3 G3 and §9.1 (WP-121, review F2): under a rule table that is not
// empty, the `unverifiable` shard of G3 is an `ok` shard: it has
// `binding` and `rule_use` (step 1's, under the table's `rules_id`).
#[test]
fn g3_under_rules_keeps_the_sections() {
    g3_case("rules", "int a = 1;\n", "int a = 2;\n", true);
}

fn g3_case(case: &str, one: &'static str, two: &'static str, under_rules: bool) {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    let tree = Tree::new(&format!("g3-{case}"), &[("a.cpp", "int a = 0;\nint b = 0;\n")]);
    write(&tree.post.join("a.cpp"), "#include \"f.h\"\nint b = 1;\n");
    write(&tree.post.join("f.h"), one);
    let o = tree.observer();
    tree.map(&o);
    let stop = Arc::new(AtomicBool::new(false));
    let (dir, target, flag) = (tree.base.clone(), tree.post.join("f.h"), stop.clone());
    let writer = std::thread::spawn(move || {
        let mut n = 0u64;
        while !flag.load(Ordering::SeqCst) {
            n += 1;
            let tmp = dir.join("f.h.tmp");
            std::fs::write(&tmp, if n.is_multiple_of(2) { one } else { two }).unwrap();
            std::fs::rename(&tmp, &target).unwrap();
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
    });
    let params = if under_rules {
        tree.params(declared_of(&tree, rename_ns("fresh", "old"), &[]))
    } else {
        tree.params(Declared::none())
    };
    let mut seen = None;
    for attempt in 0..200 {
        let v = tree.verify_with(&o, &params).unwrap();
        assert_ne!(kind(&v), VerdictKind::Verified, "{case}");
        if kind(&v) == VerdictKind::Unverifiable {
            assert_eq!(reason(&v), "post-image: changed during the job", "{case}");
            if under_rules {
                check_sections_under_rules(&v, &params);
            }
            assert_eq!(counter(&v, "verify.reparsed"), Some(2));
            check_post_shard(&v);
            seen = Some(attempt);
            break;
        }
        assert_eq!(kind(&v), VerdictKind::Mismatch, "{case}");
    }
    stop.store(true, Ordering::SeqCst);
    writer.join().unwrap();
    assert!(seen.is_some(), "{case}: G3 never seen");
}

// The worker (`spec/exec.md` §8.4): `PhaseTask` by the job's phase;
// a verify job's params from `job.post` and the store.
#[test]
fn worker_phase_task() {
    let tree = Tree::new("worker", &[("a.cpp", OVERLOAD_PRE)]);
    write(&tree.post.join("a.cpp"), OVERLOAD_POST);
    let o = tree.observer();
    let mut task = PhaseTask::new(1, 0);
    let map = tree.job(&o, Phase::Map, 0);
    let shard = task.run(&map).unwrap();
    assert!(shard.verdict.is_none());
    tree.store.publish_shard(&map.plan, Phase::Map, 0, &o.id, &shard.to_bytes()).unwrap();
    let declared = Declared::none().to_bytes();
    let declared_id = tree.store.publish_declared(&declared).unwrap();
    let post = Post { pre: 0, declared: declared_id, image: tree.image().to_post() };
    let mut job = tree.job(&o, Phase::Verify, 1);

    // Jobs that do not fit their phase.
    assert_eq!(task.run(&job).unwrap_err(), "a verify job without a post");
    let mut with_transform = job.clone();
    with_transform.post = Some(post.clone());
    with_transform.transform = Some(Digest([1; 32]));
    assert_eq!(task.run(&with_transform).unwrap_err(), "a verify job with a transform");
    let mut map_with_post = map.clone();
    map_with_post.post = Some(post.clone());
    assert_eq!(task.run(&map_with_post).unwrap_err(), "a map job with a post");

    // The declared changes: missing, not a DECL, another id.
    let missing = Post { declared: Digest([3; 32]), ..post.clone() };
    job.post = Some(missing);
    let e = task.run(&job).unwrap_err();
    assert!(
        e.starts_with(&format!("the declared changes {} are not in the store", Digest([3; 32]))),
        "{e}"
    );
    let other = Digest([4; 32]);
    std::fs::write(tree.store.declared_path(&other), shard.to_bytes()).unwrap();
    job.post = Some(Post { declared: other, ..post.clone() });
    let e = task.run(&job).unwrap_err();
    assert!(e.contains("is not a valid DECL file"), "{e}");
    std::fs::write(tree.store.declared_path(&other), &declared).unwrap();
    let e = task.run(&job).unwrap_err();
    assert!(e.ends_with(&format!("has the content id {declared_id}, not {other}")), "{e}");

    // The overlay image.
    job.post = Some(Post { image: PostImage::Overlay(Digest([2; 32])), ..post.clone() });
    assert_eq!(task.run(&job).unwrap_err(), "the overlay image needs WP-074");

    // A good job: the worker's `--findings 1 --locate 0`.
    job.post = Some(post.clone());
    let v = task.run(&job).unwrap();
    v.validate().unwrap();
    let verdict = v.verdict.as_ref().unwrap();
    assert_eq!(verdict.kind, VerdictKind::Mismatch);
    assert!(verdict.findings.len() <= 1 && verdict.findings.iter().all(|f| f.name.is_empty()));
    assert_eq!(counter(&v, "verify.reparsed"), Some(1));
    // Again, with the params kept; with another pre round; with another
    // image (the identity: the subject against itself), not the kept one.
    assert_eq!(task.run(&job).unwrap(), v);
    job.post = Some(Post { pre: 9, ..post.clone() });
    assert_eq!(reason(&task.run(&job).unwrap()), "pre-image: no map shard");
    job.post = Some(Post { image: Image::identity().to_post(), ..post });
    assert_eq!(kind(&task.run(&job).unwrap()), VerdictKind::Verified);
}

// The worker (`spec/exec.md` §8.4, `spec/verify.md` §9.1; WP-121, review
// F1): under a `DECL` container with a rule, the shard's `binding` is
// made from the job's `post`: `declared` its content id, `pre` its round.
#[test]
fn worker_binding_from_the_post() {
    let tree = Tree::new("worker-rules", &[("a.cpp", RELOCATE_PRE)]);
    write(&tree.post.join("a.cpp"), RELOCATE_POST);
    let o = tree.observer();
    let mut task = PhaseTask::new(64, 0);
    let map = tree.job(&o, Phase::Map, 0);
    let shard = task.run(&map).unwrap();
    tree.store.publish_shard(&map.plan, Phase::Map, 0, &o.id, &shard.to_bytes()).unwrap();
    let declared = declared_of(&tree, rename_ns("fresh", "old"), &[]);
    let declared_id = tree.store.publish_declared(&declared.to_bytes()).unwrap();
    let post = Post { pre: 0, declared: declared_id, image: tree.image().to_post() };
    let mut job = tree.job(&o, Phase::Verify, 1);
    for pre in [0, 9] {
        job.post = Some(Post { pre, ..post.clone() });
        let v = task.run(&job).unwrap();
        v.validate().unwrap();
        let want = if pre == 0 { VerdictKind::Verified } else { VerdictKind::Unverifiable };
        assert_eq!(kind(&v), want, "{}", reason(&v));
        let binding = slip_model::Binding { overlay: None, declared: declared_id, pre };
        assert_eq!(v.binding, Some(binding));
        assert_eq!(v.rule_use.as_ref().map(|u| u.rules), Some(declared.rules_id()));
    }
}

// §5.2: `verify` itself refuses a job of phase map and one with a
// transform.
#[test]
fn s5_2_verify_refuses_other_jobs() {
    let tree = Tree::new("refuse", &[("a.cpp", "int a = 1;\n")]);
    let o = tree.observer();
    let params = tree.params(Declared::none());
    let job = tree.job(&o, Phase::Map, 1);
    assert_eq!(
        verify(&tree.store, &job, &params).unwrap_err(),
        "the verify task runs phase verify only; the job is of phase map"
    );
    let mut job = tree.job(&o, Phase::Verify, 1);
    job.transform = Some(Digest([1; 32]));
    assert_eq!(verify(&tree.store, &job, &params).unwrap_err(), "a verify job with a transform");
}

/// The wrong-`verified` sweep: small trees, and single-token edits of
/// each. `(tree, pre text, from, to)`: the post-image is the pre text with
/// the first `from` replaced by `to`.
const SWEEP: &[(&str, &str, &str, &str)] = &[
    ("calls", CALLS, "g(1)", "g(2)"),
    ("calls", CALLS, "g(1)", "g(1L)"),
    ("calls", CALLS, "return g(1)", "return n::g(1)"),
    ("calls", CALLS, "a = 3", "a = 4"),
    ("calls", CALLS, "int f()", "long f()"),
    ("calls", CALLS, "k()", "k(3)"),
    ("struct", STRUCT, "a = 1", "a = 2"),
    ("struct", STRUCT, "short b", "long b"),
    ("struct", STRUCT, "return a;", "return b;"),
    ("struct", STRUCT, "S s", "const S s"),
    ("templates", TEMPLATES, "id(1)", "id(2)"),
    ("templates", TEMPLATES, "id(1)", "id(1L)"),
    ("templates", TEMPLATES, "T t;", "T t; int u;"),
    ("adl", ADL, "h(2)", "h(3)"),
    ("adl", ADL, "return h(x)", "return ::h(x)"),
    ("adl", ADL, "return h(x)", "return a::h(x)"),
    ("adl", ADL, "+ r(x)", "+ a::r(x)"),
    ("adl", ADL, "+ q(y)", "+ ::q(y)"),
    ("struct", STRUCT, "use(S s)", "use(::S s)"),
    ("adl", ADL, "struct X {}", "struct X { int q; }"),
    ("adl", ADL, "int h(long)", "int h(int)"),
    ("constants", CONSTANTS, "k = 3", "k = 4"),
    ("constants", CONSTANTS, "k + e1", "k + e0"),
    ("constants", CONSTANTS, "e1 }", "e1, e2 }"),
    ("constants", CONSTANTS, "unsigned char", "signed char"),
    ("macros", MACROS, "V 1", "V 2"),
    ("macros", MACROS, "W(x) x", "W(x) (x + 1)"),
    ("header", "#include \"h.h\"\nint f() { return q(1); }\n", "q(1)", "q(1.0)"),
];

const CALLS: &str = "int g(int);\nint g(long);\nnamespace n { int g(int); }\n\
int k(int a = 3);\nint f() { return g(1) + k(); }\n";
const STRUCT: &str = "struct S { int a = 1; short b; int m() const { return a; } };\n\
int use(S s) { return s.m() + s.b; }\n";
const TEMPLATES: &str = "template <class T> T id(T t) { return t; }\n\
template <class T> struct B { T t; };\nint f() { return id(1); }\n";
const ADL: &str = "namespace a { struct X {}; int h(X); template <class T> int r(T);\n\
struct Y { operator long() const { return 0; } }; int q(Y); }\n\
int h(a::X, long);\nint h(long);\nint r(a::X);\nint q(long);\n\
int f() { a::X x; a::Y y; return h(x) + h(2) + r(x) + q(y); }\n";
const CONSTANTS: &str = "enum E { e0, e1 };\nconstexpr int k = 3;\nunsigned char c = 200;\n\
int f() { return k + e1; }\n";
const MACROS: &str = "#define V 1\n#define W(x) x\nint f() { return W(V); }\n";

/// The edits of the sweep that verify, each argued from the standard.
///
/// - `h(x)` to `a::h(x)`: the unqualified call finds `::h(a::X, long)`
///   and `::h(long)` by ordinary lookup and `a::h(X)` by argument-dependent
///   lookup ([basic.lookup.argdep]); only `a::h(X)` is viable (one
///   argument, and no conversion from `a::X` to `long`), so it is called
///   ([over.match.viable]). `a::h` is looked up in `a`
///   ([basic.lookup.qual.general]) and finds `a::h(X)` alone. The same
///   function, the same argument and the same initialisation of its
///   parameter: only the written form of the callee differs (its
///   qualifier, `DeclRefExtra.qualifier`, and the call's `adl` bit,
///   `CallExpr.bits-adl`, both `out`). Every unit of the two images is
///   equal. Before WP-045 it was a mismatch for another reason: the
///   implicit constructors of `a::X` were indeterminate (`pending.demand`)
///   on both sides, with or without an edit; no unit differed then either.
/// - `use(S s)` to `use(::S s)`: `::S` is looked up in the global
///   namespace ([basic.lookup.qual.general]) and names the class `S` that
///   `S` names; the parameter type, and so the function's type
///   ([dcl.fct]), and its body are the same. Nothing that resolves
///   through the declaration changes.
///
/// Requalifications that change the meaning, and are a mismatch: `n::g(1)`
/// (another function); `a::r(x)` (the qualified name finds only the
/// template `a::r`, where the unqualified call chose the non-template
/// `::r(a::X)`, [over.match.best] p2.4); `::q(y)` (qualifying suppresses
/// argument-dependent lookup, [basic.lookup.argdep] p1, and with it the
/// better `a::q(Y)`: `::q(long)` through a user-defined conversion is
/// called instead). `h(x)` to `::h(x)` does not parse (no `::h` is
/// viable).
const SWEEP_VERIFIES: &[(&str, &str)] = &[("adl", "return h(x)"), ("struct", "use(S s)")];

/// The requalifications above that change the meaning: each must be a
/// mismatch (`(tree, to)`).
const SWEEP_MISMATCHES: &[(&str, &str)] =
    &[("calls", "return n::g(1)"), ("adl", "+ a::r(x)"), ("adl", "+ ::q(y)")];

#[test]
fn sweep_no_wrong_verified() {
    let mut verified = Vec::new();
    for &(name, pre, from, to) in SWEEP {
        assert!(pre.contains(from), "{name}: {from:?}");
        let post = pre.replacen(from, to, 1);
        let mut files = vec![("a.cpp", pre)];
        if name == "header" {
            files.push(("h.h", "int q(int);\nint q(double);\n"));
        }
        let tree = Tree::new(&format!("sweep-{name}"), &files);
        write(&tree.post.join("a.cpp"), &post);
        let o = tree.observer();
        let m = tree.map(&o);
        assert_eq!(m.outcome.status, Status::Ok, "{name}: {:?}", m.outcome);
        let v = tree.verify(&o);
        let got = v.verdict.as_ref().map(|v| (v.kind, v.reason.clone()));
        eprintln!("SWEEP {name} {from:?} -> {to:?}: {:?} {got:?}", v.outcome.status);
        if SWEEP_MISMATCHES.contains(&(name, to)) {
            assert_eq!(got.as_ref().map(|g| g.0), Some(VerdictKind::Mismatch), "{name}: {to:?}");
        }
        if got.as_ref().is_some_and(|(k, _)| *k == VerdictKind::Verified) {
            verified.push((name, from));
        }
    }
    assert_eq!(verified, SWEEP_VERIFIES, "edits that verify");
}

// §5.2 step 1 (A7): the post-image is digested under the declared rules.
// A renamed namespace under its rule is verified; without it, mismatch.
#[test]
fn s5_2_rules_applied_to_the_post_image() {
    let pre = "namespace old { int f() { return 1; } }\nint g() { return old::f(); }\n";
    let post = "namespace fresh { int f() { return 1; } }\nint g() { return fresh::f(); }\n";
    let tree = Tree::new("rules-post", &[("a.cpp", pre)]);
    write(&tree.post.join("a.cpp"), post);
    let o = tree.observer();
    tree.map(&o);
    let declared = Declared::new(vec![rename_ns("fresh", "old")], vec![], vec![], vec![]).unwrap();
    let declared = declared.with_files(Some(tree.files(&[])));
    let with = tree.verify_with(&o, &tree.params(declared.clone())).unwrap();
    assert_eq!(kind(&with), VerdictKind::Verified);
    assert_eq!(with.verdict.as_ref().unwrap().rules, declared.rules_id());
    assert_ne!(declared.rules_id(), declared.rules().id());
    assert_eq!(kind(&tree.verify(&o)), VerdictKind::Mismatch);
    // WP-110: the post walk takes the run's files. With `a.cpp` read-only
    // the namespace is a dependency entity, no rule maps it: mismatch.
    // Without files every file is read-only (knowing less never accepts
    // more): mismatch.
    let read_only = declared.clone().with_files(Some(tree.files(&["a.cpp"])));
    let v = tree.verify_with(&o, &tree.params(read_only)).unwrap();
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
    let v = tree.verify_with(&o, &tree.params(declared.with_files(None))).unwrap();
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// §5.2 step 4 (B6): the pre-image is digested with no rules. Two
// namespaces swapped under rules and one real difference: one finding.
// Rules applied to the pre-image would swap it too (R2, or more findings).
#[test]
fn s5_2_pre_image_without_rules_at_step_4() {
    let pre = "namespace aa { int f() { return 1; } }\nnamespace bb { int h() { return 2; } }\n\
               int g() { return aa::f() + bb::h(); }\nint k() { return 1; }\n";
    let post = "namespace bb { int f() { return 1; } }\nnamespace aa { int h() { return 2; } }\n\
                int g() { return bb::f() + aa::h(); }\nint k() { return 2; }\n";
    let tree = Tree::new("rules-pre", &[("a.cpp", pre)]);
    write(&tree.post.join("a.cpp"), post);
    let o = tree.observer();
    tree.map(&o);
    let rules = RuleTable::new(vec![rename_ns("bb", "aa"), rename_ns("aa", "bb")]).unwrap();
    let declared = Declared::new(rules.rules().to_vec(), vec![], vec![], vec![]).unwrap();
    let declared = declared.with_files(Some(tree.files(&[])));
    let v = tree.verify_with(&o, &tree.params(declared)).unwrap();
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
    assert_eq!(v.verdict.as_ref().unwrap().total, 1);
}

// §5.1 (B4): `directory` is rewritten. An include directory relative to
// it, in a word the rewrite does not resolve (`-Wp,`), then names the
// post tree: the edited header is compared (with `directory` kept, the
// post parse would read the subject's, and G1 refuse).
#[test]
fn s5_1_directory_rewritten() {
    let tree = Tree::new(
        "dir",
        &[("a.cpp", "#include <h.h>\nint f() { return k; }\n"), ("inc/h.h", "const int k = 1;\n")],
    );
    write(&tree.post.join("inc/h.h"), "const int k = 2;\n");
    let o = tree.observer_of("a.cpp", &["-Wp,-Iinc"]);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// F1 (review): a failed `__has_include` into the subject through `-I`
// relative to a `directory` outside every root. The rewrite resolves the
// word to the post tree, where the edit added the header: mismatch, as the
// truth is.
#[test]
fn f1_failed_probe_into_the_subject() {
    let main =
        "#if __has_include(<x.h>)\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
    let tree = Tree::new("f1", &[("a.cpp", main)]);
    std::fs::create_dir_all(tree.subject.join("inc")).unwrap();
    write(&tree.post.join("inc/x.h"), "\n");
    let o = tree.observer_in(&tree.base.join("build"), &["-I../s/inc".to_owned()]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
    assert!(tree.truth_differs(&o, &m));
}

// F1b (review): the same word, shadowing: the edit adds `inc/x.h`, which in
// the post-image comes before `ext/x.h`.
#[test]
fn f1b_shadowing_header_added() {
    let tree = Tree::new("f1b", &[("a.cpp", "#include <x.h>\nint f() { return X; }\n")]);
    std::fs::create_dir_all(tree.subject.join("inc")).unwrap();
    write(&tree.base.join("ext/x.h"), "#define X 1\n");
    write(&tree.post.join("inc/x.h"), "#define X 2\n");
    let ext = arg("-I", &tree.base.join("ext"));
    let o = tree.observer_in(&tree.base.join("build"), &["-I../s/inc".to_owned(), ext]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
    assert!(tree.truth_differs(&o, &m));
}

// §5.3 L1: F1 through a word the rewrite does not resolve (`-Wp,`): the
// failed probe looked in a directory of the subject; refused, naming it.
#[test]
fn l1_failed_probe_through_a_word_not_resolved() {
    let main =
        "#if __has_include(<x.h>)\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
    let tree = Tree::new("l1-probe", &[("a.cpp", main)]);
    std::fs::create_dir_all(tree.subject.join("inc")).unwrap();
    write(&tree.post.join("inc/x.h"), "\n");
    let o = tree.observer_in(&tree.base.join("build"), &["-Wp,-I../s/inc".to_owned()]);
    tree.map(&o);
    let v = tree.verify(&o);
    let s = tree.subject.to_str().unwrap();
    assert_eq!(
        reason(&v),
        format!("post-image: searches ../s/inc: {s} is a directory of the subject")
    );
    check_post_shard(&v);
}

// §5.3 L1: a lookup that succeeded, after trying a place in the subject
// reached through a symbolic link of an include directory outside both
// roots (`a/sub` -> `s/x`). The list's directories are outside the
// subject; only the place tried shows it. The edit adds `x/q.h`, which in
// the post-image is found first.
#[test]
fn l1_successful_lookup_tried_the_subject() {
    let tree = Tree::new("l1-tried", &[("a.cpp", "#include <sub/q.h>\nint f() { return Q; }\n")]);
    std::fs::create_dir_all(tree.subject.join("x")).unwrap();
    std::fs::create_dir_all(tree.post.join("x")).unwrap();
    std::fs::create_dir_all(tree.base.join("a")).unwrap();
    std::os::unix::fs::symlink(tree.subject.join("x"), tree.base.join("a/sub")).unwrap();
    write(&tree.base.join("b/sub/q.h"), "#define Q 1\n");
    write(&tree.post.join("x/q.h"), "#define Q 2\n");
    let flags = [arg("-I", &tree.base.join("a")), arg("-I", &tree.base.join("b"))];
    let o = tree.observer_in(&tree.subject, &flags);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    let a = tree.base.join("a");
    assert_eq!(
        reason(&v),
        format!(
            "post-image: looked for sub/q.h in {}: {} is a directory of the subject",
            a.display(),
            tree.subject.display()
        )
    );
    assert!(tree.truth_differs(&o, &m));
}

// §5.3 L1: a lookup from a header outside both roots (`ext/e.h`) into the
// subject by a relative name; the search list is outside the subject, the
// includer's directory is where the name is looked for.
#[test]
fn l1_lookup_from_a_file_outside_both_roots() {
    let main = "#include <e.h>\nint f() { return E; }\n";
    let tree = Tree::new("l1-outside", &[("a.cpp", main)]);
    std::fs::create_dir_all(tree.subject.join("inc")).unwrap();
    write(
        &tree.base.join("ext/e.h"),
        "#if __has_include(\"../s/inc/z.h\")\n#define E 2\n#else\n#define E 1\n#endif\n",
    );
    write(&tree.post.join("inc/z.h"), "\n");
    let ext = tree.base.join("ext");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &ext)]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(
        reason(&v),
        format!(
            "post-image: looked for ../s/inc/z.h in {}: {} is a directory of the subject",
            ext.display(),
            tree.subject.display()
        )
    );
    assert!(tree.truth_differs(&o, &m));
}

// §5.3 L1: the includer's directory, on its own. The probe is in
// `ext/sub/y.h`; `ext/sub` is not in the search list, and the name from
// `ext` (the list) is outside both roots: only the includer's directory
// reaches the subject.
#[test]
fn l1_includer_directory_only() {
    let main = "#include <e.h>\nint f() { return E; }\n";
    let tree = Tree::new("l1-includer", &[("a.cpp", main)]);
    std::fs::create_dir_all(tree.subject.join("inc")).unwrap();
    write(&tree.base.join("ext/e.h"), "#include \"sub/y.h\"\n");
    write(
        &tree.base.join("ext/sub/y.h"),
        "#if __has_include(\"../../s/inc/z.h\")\n#define E 2\n#else\n#define E 1\n#endif\n",
    );
    write(&tree.post.join("inc/z.h"), "\n");
    let ext = tree.base.join("ext");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &ext)]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    let r = reason(&v);
    assert!(r.starts_with("post-image: looked for ../../s/inc/z.h in "), "{r}");
    assert!(r.ends_with(&format!("sub: {} is a directory of the subject", tree.subject.display())));
    assert!(tree.truth_differs(&o, &m));
}

// F2 (review), §5.3 G4: a response file the driver would read is a word
// slip cannot examine; refused, the map job unchanged. Relative from a
// directory outside the roots, and absolute in the subject.
#[test]
fn g4_response_file() {
    let tree = Tree::new("g4", &[("a.cpp", "int f() { return V; }\n"), ("flags.rsp", "-DV=1\n")]);
    write(&tree.post.join("flags.rsp"), "-DV=2\n");
    for (dir, word) in [
        (tree.base.join("build"), "@../s/flags.rsp".to_owned()),
        (tree.subject.clone(), arg("@", &tree.subject.join("flags.rsp"))),
    ] {
        let o = tree.observer_in(&dir, std::slice::from_ref(&word));
        let m = tree.map(&o);
        assert_eq!(m.outcome.status, Status::Ok, "{:?}", m.outcome);
        let v = tree.verify(&o);
        let shown = image_word(&tree, &word);
        assert_eq!(
            reason(&v),
            format!(
                "post-image: the command names the response file {shown}, whose words slip cannot examine"
            )
        );
    }
}

/// `word` as the post command writes it (the textual rule).
fn image_word(tree: &Tree, word: &str) -> String {
    word.replace(tree.subject.to_str().unwrap(), tree.post.to_str().unwrap())
}

// F3 (review): an include directory spelled with another case than the
// subject root, on a file system that ignores case. The rewrite resolves
// it by identity, and the post prefix it gives is the main file's, spelled
// another way: guard G5 (the truth differs). Skipped, visibly, where the
// file system is case-sensitive.
#[test]
fn f3_case_insensitive_spelling() {
    let tree = Tree::new(
        "f3",
        &[("a.cpp", "#include <h.h>\nint f() { return k; }\n"), ("inc/h.h", "const int k = 1;\n")],
    );
    write(&tree.post.join("inc/h.h"), "const int k = 2;\n");
    let upper = tree.base.join("S").join("inc");
    if std::fs::metadata(&upper).is_err() {
        eprintln!("f3_case_insensitive_spelling: SKIPPED, the file system is case-sensitive");
        return;
    }
    let o = tree.observer_in(&tree.subject, &[arg("-I", &upper)]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    let g5 = format!(
        "post-image: -I{} and {} spell the files under",
        upper.display(),
        tree.path("a.cpp")
    );
    assert!(reason(&v).starts_with(&g5), "{}", reason(&v));
    // Through a word the rewrite does not resolve: G1, by identity.
    let o2 = tree.observer_in(&tree.subject, &[arg("-Wp,-I", &upper)]);
    tree.map(&o2);
    let v2 = tree.verify(&o2);
    assert!(reason(&v2).starts_with("post-image: read "), "{}", reason(&v2));
    assert!(reason(&v2).ends_with("/inc/h.h, a file of the subject"), "{}", reason(&v2));
    assert!(tree.truth_differs(&o, &m));
}

// F3, the symbolic-link form (runs everywhere): `link` -> `s`. Resolved by
// the rewrite, to the post prefix the main file has by another spelling:
// G5 (F6s of the re-check); through a word it does not resolve: L1.
#[test]
fn f3_symbolic_link_to_the_subject() {
    let tree = Tree::new(
        "f3link",
        &[("a.cpp", "#include <h.h>\nint f() { return k; }\n"), ("inc/h.h", "const int k = 1;\n")],
    );
    write(&tree.post.join("inc/h.h"), "const int k = 2;\n");
    std::os::unix::fs::symlink(&tree.subject, tree.base.join("link")).unwrap();
    let inc = tree.base.join("link/inc");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &inc)]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    let g5 =
        format!("post-image: -I{} and {} spell the files under", inc.display(), tree.path("a.cpp"));
    assert!(reason(&v).starts_with(&g5), "{}", reason(&v));
    let o2 = tree.observer_in(&tree.subject, &[arg("-Wp,-I", &inc)]);
    tree.map(&o2);
    let v2 = tree.verify(&o2);
    assert_eq!(kind(&v2), VerdictKind::Unverifiable);
    assert!(tree.truth_differs(&o, &m));
}

// §5.1: a word the rewrite resolved keeps its spelling: `__FILE__` in a
// header found through `-I../s/inc` from outside the roots is `../s/inc/h.h`
// in the post-image too (the added `-fmacro-prefix-map=` word). No edit:
// verified at the sums.
#[test]
fn s5_1_resolved_word_keeps_its_spelling() {
    let h = "static_assert(sizeof(__FILE__) == sizeof(\"../s/inc/h.h\"));\nconst int k = 1;\n";
    let tree = Tree::new(
        "spelling",
        &[("a.cpp", "#include <h.h>\nint f() { return k; }\n"), ("inc/h.h", h)],
    );
    let o = tree.observer_in(&tree.base.join("build"), &["-I../s/inc".to_owned()]);
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Ok, "{:?}", m.outcome);
    let v = tree.verify(&o);
    assert_eq!(v.outcome.status, Status::Ok, "{:?}", v.outcome);
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
}

// F4 (review), §5.3 O2 for the tree form: a header outside both roots
// changed between the map phase and the job (same size), with a post edit
// that compensates. Refused as G2 refuses a changed input, before the sums.
#[test]
fn o2_outside_drift_at_the_sums() {
    let pre = "#include <cfg.h>\n#if USE_NEW\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
    let post = "#include <cfg.h>\n#if !USE_NEW\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
    let tree = Tree::new("o2", &[("a.cpp", pre)]);
    write(&tree.post.join("a.cpp"), post);
    let cfg = tree.base.join("ext/cfg.h");
    write(&cfg, "#define USE_NEW 0\n");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &tree.base.join("ext"))]);
    tree.map(&o);
    write(&cfg, "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), format!("pre-image: {} changed since the map phase", cfg.display()));
    check_post_shard(&v);
}

/// The F6 tree of the re-check: `x.h` in `s/inc` holds `head` and an array
/// whose length is that of `spelling` (a file's spelling, as a constant);
/// the pre-image includes `<x.h>` through `-I../s/inc` from `build/`, the
/// edit `"inc/x.h"` from the includer's directory. The main file is
/// absolute, or `../s/a.cpp` when `relative_main`. The verdict and whether
/// the truth differs.
fn two_routes(name: &str, head: &str, spelling: &str, relative_main: bool) -> (Shard, bool) {
    let xh = format!("{head}\ninline char where[__builtin_strlen({spelling}) + 1] = {{}};\n");
    let tree = Tree::new(
        name,
        &[("a.cpp", "#include <x.h>\nint f() { return sizeof(where); }\n"), ("inc/x.h", &xh)],
    );
    write(&tree.post.join("a.cpp"), "#include \"inc/x.h\"\nint f() { return sizeof(where); }\n");
    let mut o = tree.observer_in(&tree.base.join("build"), &["-I../s/inc".to_owned()]);
    if relative_main {
        *o.command.argv.last_mut().unwrap() = b"../s/a.cpp".to_vec();
        o.id = o.command.observer_id();
    }
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Ok, "{:?}", m.outcome);
    let v = tree.verify(&o);
    let truth = tree.truth_differs(&o, &m);
    (v, truth)
}

/// A hand-written `std::source_location` of the shape Kiln's
/// `__builtin_source_location` asks for.
const SOURCE_LOCATION: &str = "namespace std { struct source_location {
  struct __impl { const char* _M_file_name; const char* _M_function_name; unsigned _M_line; unsigned _M_column; };
  static consteval source_location current(decltype(__builtin_source_location()) p = __builtin_source_location()) noexcept {
    source_location s; s.p_ = static_cast<const __impl*>(p); return s; }
  constexpr const char* file_name() const noexcept { return p_->_M_file_name; }
  const __impl* p_ = nullptr;
}; }";

// F6 (re-check), §5.1 1(b): two routes to one file, `-I../s/inc` and the
// includer's directory. The relative word stays relative
// (`../post-tree/inc`), so its post prefix is not related to the main
// file's (`<post>/`) and each route keeps its own spelling: the edit's
// change of spelling is seen, mismatch, as the truth is. Each channel of a
// file's spelling (§5.1, "Spellings"): `__FILE__`, `__builtin_FILE()`,
// `std::source_location::current().file_name()` show the change;
// `__FILE_NAME__` (the last component) and `__BASE_FILE__` (the main
// file's) have none to show, and verify, as the truth does not differ.
#[test]
fn f6_two_routes_to_one_file() {
    for (name, head, spelling) in [
        ("f6-file", "", "__FILE__"),
        ("f6-builtin", "", "__builtin_FILE()"),
        ("f6-srcloc", SOURCE_LOCATION, "std::source_location::current().file_name()"),
    ] {
        let (v, truth) = two_routes(name, head, spelling, false);
        assert_eq!(kind(&v), VerdictKind::Mismatch, "{name}: {}", reason(&v));
        assert!(truth, "{name}");
    }
    for (name, spelling, relative_main) in [
        ("f6-file-name", "__FILE_NAME__", false),
        ("f6-base-file", "__BASE_FILE__", false),
        ("f6-base-file-rel", "__BASE_FILE__", true),
    ] {
        let (v, truth) = two_routes(name, "", spelling, relative_main);
        assert_eq!(kind(&v), VerdictKind::Verified, "{name}: {}", reason(&v));
        assert!(!truth, "{name}");
    }
}

// F6s (re-check), §5.3 G5: the same two routes, the include directory
// through a symbolic link to the subject (absolute). Its post prefix is
// the main file's, `<post>/`, written two ways: refused, naming both words.
#[test]
fn f6s_two_routes_through_a_link() {
    let xh = "inline char where[sizeof(__FILE__)] = {};\n";
    let tree = Tree::new(
        "f6s",
        &[("a.cpp", "#include <x.h>\nint f() { return sizeof(where); }\n"), ("inc/x.h", xh)],
    );
    write(&tree.post.join("a.cpp"), "#include \"inc/x.h\"\nint f() { return sizeof(where); }\n");
    std::os::unix::fs::symlink(&tree.subject, tree.base.join("link")).unwrap();
    let inc = tree.base.join("link/inc");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &inc)]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    let (b, sp, pp) = (tree.base.display(), tree.subject.display(), tree.post.display());
    assert_eq!(
        reason(&v),
        format!(
            "post-image: -I{b}/link/inc and {sp}/a.cpp spell the files under {pp}/ differently \
             ({pp}/={b}/link/ and {pp}/={sp}/)"
        )
    );
    assert!(tree.truth_differs(&o, &m));
}

// Z1 (re-check of fix round 2), §5.3 G5 (e): a word spelled through the
// subject that leaves it, by `..`, to a directory under no root. Its pair
// would map every spelling under that directory, also one reached through
// another word (`-I<b>/s2`, kept as written): refused, naming the word.
#[test]
fn z1_word_leaves_the_subject() {
    let xh = "inline char where[sizeof(__FILE__)] = {};\n";
    let tree = Tree::new("z1", &[("a.cpp", "#include <x.h>\nint f() { return sizeof(where); }\n")]);
    write(&tree.base.join("s2/inc/x.h"), xh);
    write(&tree.post.join("a.cpp"), "#include <inc/x.h>\nint f() { return sizeof(where); }\n");
    let up = tree.subject.join("../s2/inc");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &up), arg("-I", &tree.base.join("s2"))]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert_eq!(reason(&v), leaves(&up, &tree.base));
    assert!(tree.truth_differs(&o, &m));
}

// Z2: the same word alone, the post-image naming the header by its
// absolute path: refused.
#[test]
fn z2_word_leaves_the_subject_absolute_include() {
    let xh = "inline char where[sizeof(__FILE__)] = {};\n";
    let tree = Tree::new("z2", &[("a.cpp", "#include <x.h>\nint f() { return sizeof(where); }\n")]);
    let header = tree.base.join("s2/inc/x.h");
    write(&header, xh);
    let post = format!("#include \"{}\"\nint f() {{ return sizeof(where); }}\n", header.display());
    write(&tree.post.join("a.cpp"), &post);
    let up = tree.subject.join("../s2/inc");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &up)]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert_eq!(reason(&v), leaves(&up, &tree.base));
    assert!(tree.truth_differs(&o, &m));
}

/// G5's reason (e) for the word `-I<up>`, whose post prefix is `<b>/s2/inc/`.
fn leaves(up: &Path, b: &Path) -> String {
    format!(
        "post-image: -I{} is spelled through the subject and leaves it ({}/s2/inc/ is under no \
         post root)",
        up.display(),
        b.display()
    )
}

// Z controls: a word that goes through `..` and stays in the subject.
// Relative, from the subject: its pair `inc/=inc/../inc/` is under the post
// root; no edit verifies, an edit is a mismatch. Absolute: its pair
// `<post>/inc/=<s>/inc/../inc/` disagrees with the root pair `<post>/=<s>/`
// (a file under `<post>/inc` reached from the main file's directory would
// be spelled through the word): G5 (b), as before this round.
#[test]
fn z_control_word_stays_in_the_subject() {
    let xh = "inline char where[sizeof(__FILE__)] = {};\n";
    let main = "#include <x.h>\nint f() { return sizeof(where); }\n";
    let tree = Tree::new("zc", &[("a.cpp", main), ("inc/x.h", xh)]);
    let o = tree.observer_in(&tree.subject, &["-Iinc/../inc".to_owned()]);
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Ok, "{:?}", m.outcome);
    assert_eq!(kind(&tree.verify(&o)), VerdictKind::Verified, "{}", reason(&tree.verify(&o)));
    write(&tree.post.join("a.cpp"), "#include <x.h>\nint f() { return sizeof(where) + 1; }\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
    let inc = tree.subject.join("inc/../inc");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &inc)]);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    let (sp, pp) = (tree.subject.display(), tree.post.display());
    assert!(
        reason(&v).ends_with(&format!(
            "spell the files under {pp}/inc/ differently ({pp}/={sp}/ and {pp}/inc/={}/)",
            inc.display()
        )),
        "{}",
        reason(&v)
    );
}

// Z, another root: a word leaving one subject root by `..` into another.
// It is rewritten to the other root's post tree, and its pair
// `<post3>/inc/=<s>/../s3/inc/` disagrees with that root's pair
// `<post3>/=<s3>/` (the files under `<post3>/inc` reached by another route
// are spelled `<s3>/inc/...` in the real post-image): G5 (b).
#[test]
fn z_word_leaves_into_another_root() {
    let xh = "inline char where[sizeof(__FILE__)] = {};\n";
    let tree = Tree::new("zr", &[("a.cpp", "#include <x.h>\nint f() { return sizeof(where); }\n")]);
    let (s3, p3) = (tree.base.join("s3"), tree.base.join("post3"));
    write(&s3.join("inc/x.h"), xh);
    write(&p3.join("inc/x.h"), xh);
    let image = Image::new(vec![
        RootMap { subject: bytes(&tree.subject), post: bytes(&tree.post) },
        RootMap { subject: bytes(&s3), post: bytes(&p3) },
    ])
    .unwrap();
    let up = tree.subject.join("../s3/inc");
    let o = tree.observer_in(&tree.subject, &[arg("-I", &up)]);
    let post = image.post_command(&o.command);
    assert_eq!(post.command.argv[2], arg("-I", &p3.join("inc")).into_bytes());
    tree.map(&o);
    let params = Params { image, ..tree.params(Declared::none()) };
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    let (s3, p3) = (s3.display(), p3.display());
    assert!(
        reason(&v).ends_with(&format!(
            "spell the files under {p3}/inc/ differently ({p3}/={s3}/ and {p3}/inc/={}/)",
            up.display()
        )),
        "{}",
        reason(&v)
    );
}

// A3 (re-check of fix round 3), §5.3 G5 (e): the command's directory
// `<s>/a/w` is a directory in the subject; the edit makes `<post>/a/w` a
// link to `<b>/z/w`. The word `-I../../../s2/inc` names `<b>/s2/inc` from
// the subject's directory; the rewrite writes it from the post
// directory's real place, `../../s2/inc`, whose pair is, by real paths,
// under no post root: refused. The real post-image (the link in the
// subject's place) does not find the header at all.
#[test]
fn a3_post_directory_becomes_a_link() {
    let xh = "inline char where[sizeof(__FILE__)] = {};\n";
    let main = "#include <x.h>\nint f() { return sizeof(where); }\n";
    let tree = Tree::new("a3", &[("a.cpp", main)]);
    write(&tree.base.join("s2/inc/x.h"), xh);
    std::fs::create_dir_all(tree.base.join("z/w")).unwrap();
    std::fs::create_dir_all(tree.post.join("a")).unwrap();
    std::os::unix::fs::symlink(tree.base.join("z/w"), tree.post.join("a/w")).unwrap();
    let o = tree.observer_in(&tree.subject.join("a/w"), &["-I../../../s2/inc".to_owned()]);
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Ok, "{:?}", m.outcome);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&v),
        "post-image: -I../../../s2/inc is spelled through the subject and leaves it \
         (../../s2/inc/ is under no post root)"
    );
    // The truth: the link in the subject's place.
    std::fs::remove_dir_all(tree.subject.join("a/w")).unwrap();
    std::os::unix::fs::symlink(tree.base.join("z/w"), tree.subject.join("a/w")).unwrap();
    let now = MapTask::new().run(&tree.job(&o, Phase::Map, 0)).unwrap();
    assert_ne!(now.outcome.status, Status::Ok, "the real post-image finds no x.h");
}

// A3, the link deeper in the word: the directory `<s>/a` is the same in
// both trees; `-Iw/../inc` walks `<s>/a/w`, a directory in the subject and
// a link to `<b>/z/w` in the post tree. The rewrite (`inc`, from the
// subject's walk) stays in the post root, so G5 (e) holds; the real
// post-image reads `<b>/z/inc/x.h` instead: G5 (f).
#[test]
fn a3_link_deeper_in_the_word() {
    let main = "#include <x.h>\nint f() { return k; }\n";
    let tree = Tree::new("a3w", &[("a.cpp", main), ("a/inc/x.h", "inline int k = 1;\n")]);
    std::fs::create_dir_all(tree.subject.join("a/w")).unwrap();
    write(&tree.base.join("z/inc/x.h"), "inline int k = 2;\n");
    std::fs::create_dir_all(tree.base.join("z/w")).unwrap();
    std::os::unix::fs::symlink(tree.base.join("z/w"), tree.post.join("a/w")).unwrap();
    let o = tree.observer_in(&tree.subject.join("a"), &["-Iw/../inc".to_owned()]);
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Ok, "{:?}", m.outcome);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&v),
        format!(
            "post-image: -Iw/../inc walks {}, which is a directory in the subject and a link \
             to {} in the post tree",
            tree.subject.join("a/w").display(),
            tree.base.join("z/w").display()
        )
    );
    std::fs::remove_dir_all(tree.subject.join("a/w")).unwrap();
    std::os::unix::fs::symlink(tree.base.join("z/w"), tree.subject.join("a/w")).unwrap();
    let now = MapTask::new().run(&tree.job(&o, Phase::Map, 0)).unwrap();
    assert_eq!(now.outcome.status, Status::Ok, "{:?}", now.outcome);
    assert_ne!(now.digests, m.digests, "the real post-image reads another x.h");
}

// F6r (re-check): the two routes with no edit, from outside the roots and
// from the subject: verified (the back-maps refuse nothing they need not).
#[test]
fn f6r_two_routes_no_edit() {
    // One variable per inclusion: the file has no guard, and is included
    // twice.
    let xh = "#define CAT(a, b) a##b\n#define NAME(n) CAT(where, n)\n\
              inline char NAME(__COUNTER__)[sizeof(__FILE__)] = {};\n#undef CAT\n#undef NAME\n";
    let main = "#include <x.h>\n#include \"inc/x.h\"\n\
                int f() { return sizeof(where0) + sizeof(where1); }\n";
    let tree = Tree::new("f6r", &[("a.cpp", main), ("inc/x.h", xh)]);
    for (dir, flag) in [
        (tree.base.join("build"), "-I../s/inc".to_owned()),
        (tree.subject.clone(), "-Iinc".to_owned()),
        (tree.subject.clone(), arg("-I", &tree.subject.join("inc"))),
    ] {
        let o = tree.observer_in(&dir, std::slice::from_ref(&flag));
        let m = tree.map(&o);
        assert_eq!(m.outcome.status, Status::Ok, "{flag}: {:?}", m.outcome);
        let v = tree.verify(&o);
        assert_eq!(v.outcome.status, Status::Ok, "{flag}: {:?}", v.outcome);
        assert_eq!(kind(&v), VerdictKind::Verified, "{flag}: {}", reason(&v));
    }
}

// Q3 (re-check), §5.3 G5: the command's own prefix map over a prefix the
// rewrite maps, no edit (it was a mismatch with no reason): refused,
// naming the option, before and after the word.
#[test]
fn g5_command_prefix_map() {
    let xh = "inline char where[sizeof(__FILE__)] = {};\n";
    let tree = Tree::new(
        "g5",
        &[("a.cpp", "#include <x.h>\nint f() { return sizeof(where); }\n"), ("inc/x.h", xh)],
    );
    let user = "-fmacro-prefix-map=../s/=SRC/".to_owned();
    for flags in [[user.clone(), "-I../s/inc".to_owned()], ["-I../s/inc".to_owned(), user.clone()]]
    {
        let o = tree.observer_in(&tree.base.join("build"), &flags);
        tree.map(&o);
        let v = tree.verify(&o);
        assert_eq!(
            reason(&v),
            format!(
                "post-image: {user} maps ../s/, which overlaps ../s/ of -I../s/inc; \
                 slip does not compose prefix maps"
            ),
            "{flags:?}"
        );
    }
}

// §5.3 G5, `#line`: a name the source writes is kept by the real
// post-image, but the post command's pairs apply to it. An edit that
// writes the post prefix of `-I../s/inc` in a `#line` name would map back
// to the pre-image's name: refused. A plain change of name: mismatch.
#[test]
fn g5_line_name() {
    let main = |name: &str| {
        format!("#include <h.h>\n#line 1 \"{name}\"\ninline char where[sizeof(__FILE__)] = {{}};\n")
    };
    let tree = Tree::new("g5-line", &[("a.cpp", &main("../s/z.h")), ("inc/h.h", "\n")]);
    write(&tree.post.join("a.cpp"), &main("../post-tree/z.h"));
    let o = tree.observer_in(&tree.base.join("build"), &["-I../s/inc".to_owned()]);
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(
        reason(&v),
        "post-image: the #line name ../post-tree/z.h begins with ../post-tree/, which the post \
         command maps"
    );
    assert!(tree.truth_differs(&o, &m));
    // A name the post command does not map: no refusal.
    drop(tree);
    let tree = Tree::new("g5-line2", &[("a.cpp", &main("z.h")), ("inc/h.h", "\n")]);
    write(&tree.post.join("a.cpp"), &main("zz.h"));
    let o = tree.observer_in(&tree.base.join("build"), &["-I../s/inc".to_owned()]);
    tree.map(&o);
    assert_eq!(kind(&tree.verify(&o)), VerdictKind::Mismatch);
}

// F7 (re-check): an absolute `__has_include` of a file under the subject
// root; the edit adds the file. Kiln's lookup log does not record an
// absolute name (Kiln `spec/pp.md`), so L1 cannot see the probe: §5.1
// "What remains outside". When the log has it, `Lookups::of` lists the
// name where it is and L1 refuses, with no new code.
#[test]
#[ignore = "needs: kiln (absolute names in the lookup log)"]
fn f7_absolute_has_include() {
    let tree = Tree::new("f7", &[]);
    let main = format!(
        "#if __has_include(\"{}\")\nint f() {{ return 2; }}\n#else\nint f() {{ return 1; }}\n#endif\n",
        tree.path("new.h")
    );
    write(&tree.subject.join("a.cpp"), &main);
    write(&tree.post.join("a.cpp"), &main);
    write(&tree.post.join("new.h"), "\n");
    let o = tree.observer();
    let m = tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(
        reason(&v),
        format!(
            "post-image: looked for {} in /: {} is a directory of the subject",
            tree.path("new.h"),
            tree.subject.display()
        )
    );
    assert!(tree.truth_differs(&o, &m));
}

// ----- N1 (§5.3, work package WP-117): no file has appeared, outside the
// subject roots, where the map parse looked for one and found none.

const CFG_PRE: &str =
    "#include <cfg.h>\n#if USE_NEW\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
/// The edit that compensates `USE_NEW` going from 0 to 1.
const CFG_POST: &str =
    "#include <cfg.h>\n#if !USE_NEW\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";

fn appeared(p: &Path) -> String {
    format!("pre-image: {} appeared since the map phase", p.display())
}

/// The case of most attacks: `a.cpp` is `pre` in the subject and `post` in
/// the post tree, compiled from the subject with `-I` each of `dirs`
/// (under the tree's base); `ext/cfg.h` holds `USE_NEW 0`; the map shard
/// is published.
fn cfg_tree(name: &str, pre: &str, post: &str, dirs: &[&str]) -> (Tree, Observer, Shard) {
    cfg_tree_with(name, pre, post, dirs, |_| {})
}

/// As [`cfg_tree`], with `setup` run on the tree before the map phase.
fn cfg_tree_with(
    name: &str,
    pre: &str,
    post: &str,
    dirs: &[&str],
    setup: impl Fn(&Tree),
) -> (Tree, Observer, Shard) {
    let tree = Tree::new(name, &[("a.cpp", pre)]);
    write(&tree.post.join("a.cpp"), post);
    write(&tree.base.join("ext/cfg.h"), "#define USE_NEW 0\n");
    setup(&tree);
    let flags: Vec<String> = dirs.iter().map(|d| arg("-I", &tree.base.join(d))).collect();
    let o = tree.observer_in(&tree.subject, &flags);
    let m = tree.map(&o);
    assert_eq!(m.outcome.status, Status::Ok, "{:?}", m.outcome);
    (tree, o, m)
}

// F4b (re-check), item 5: a header added outside both roots after the map
// phase, in a directory searched before the one the map read it from, with
// an edit that compensates (the sums agree). Refused by N1, before the
// sums. The pair: without the drift the edit is seen, mismatch.
#[test]
fn f4b_outside_header_added_after_the_map() {
    let mk = |t: &Tree| std::fs::create_dir_all(t.base.join("ext2")).unwrap();
    let (tree, o, _) = cfg_tree_with("f4b", CFG_PRE, CFG_POST, &["ext2", "ext"], mk);
    write(&tree.base.join("ext2/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), appeared(&tree.base.join("ext2/cfg.h")));
    check_post_shard(&v);
}

#[test]
fn f4b_pair_without_the_drift() {
    let mk = |t: &Tree| std::fs::create_dir_all(t.base.join("ext2")).unwrap();
    let (tree, o, _) = cfg_tree_with("f4b-pair", CFG_PRE, CFG_POST, &["ext2", "ext"], mk);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// N1 is checked whatever the comparison finds: the same drift with no edit,
// so that the sums differ, is refused the same way.
#[test]
fn n1_checked_when_the_sums_differ() {
    let (tree, o, _) = cfg_tree("n1-differ", CFG_PRE, CFG_PRE, &["ext2", "ext"]);
    write(&tree.base.join("ext2/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.base.join("ext2/cfg.h")));
}

// N1 names the first path in path order, not in the order of the search:
// `ext-b` is searched first, `ext-a` is named.
#[test]
fn n1_first_in_path_order() {
    let (tree, o, _) = cfg_tree("n1-order", CFG_PRE, CFG_POST, &["ext-b", "ext-a", "ext"]);
    write(&tree.base.join("ext-b/cfg.h"), "#define USE_NEW 1\n");
    write(&tree.base.join("ext-a/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.base.join("ext-a/cfg.h")));
}

// Item 3: a file the transform added under a subject root is the edit, not
// a drift. The map parse looked for `new.h` beside `a.cpp` (a negative
// under the subject root); the file now exists there and in the post tree
// (empty: the sums agree). Not refused: verified.
#[test]
fn n1_subject_root_file_is_not_refused() {
    let main = "#if __has_include(\"new.h\")\n#include \"new.h\"\n#endif\nint f() { return 1; }\n";
    let tree = Tree::new("n1-subject", &[("a.cpp", main)]);
    let o = tree.observer();
    let m = tree.map(&o);
    let negatives = m.negatives.as_ref().expect("negatives in a map shard");
    assert!(negatives.contains(&bytes(&tree.subject.join("new.h"))), "{negatives:?}");
    write(&tree.subject.join("new.h"), "\n");
    write(&tree.post.join("new.h"), "\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
}

// Item 3, "exists" is Kiln's test: a dangling symbolic link at a negative
// is no file (Kiln's open fails): not refused; the edit is seen.
#[test]
fn n1_dangling_link_is_no_file() {
    let (tree, o, _) = cfg_tree("n1-dangling", CFG_PRE, CFG_POST, &["ext2", "ext"]);
    link("nowhere.h", &tree.base.join("ext2/cfg.h"));
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// Item 3: a directory of that name is no file (Kiln refuses to read a
// directory and searches on): not refused.
#[test]
fn n1_directory_is_no_file() {
    let (tree, o, _) = cfg_tree("n1-dir", CFG_PRE, CFG_POST, &["ext2", "ext"]);
    std::fs::create_dir_all(tree.base.join("ext2/cfg.h")).unwrap();
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// Item 3: a file that cannot be opened is no file for Kiln, which searches
// on; one that can is a file. Mode 000 cannot be opened except by root,
// who can: the test follows whichever this process is.
#[test]
fn n1_unreadable_file() {
    use std::os::unix::fs::PermissionsExt;
    let (tree, o, _) = cfg_tree("n1-unreadable", CFG_PRE, CFG_POST, &["ext2", "ext"]);
    let cfg = tree.base.join("ext2/cfg.h");
    write(&cfg, "#define USE_NEW 1\n");
    std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::File::open(&cfg).is_ok();
    let v = tree.verify(&o);
    std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o644)).unwrap();
    if readable {
        assert_eq!(reason(&v), appeared(&cfg));
    } else {
        assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
    }
}

// Item 3: a pre shard without the section, fail closed.
#[test]
fn n1_pre_shard_without_negatives() {
    let (tree, o, m) = cfg_tree("n1-none", CFG_PRE, CFG_PRE, &["ext"]);
    tree.put_map(&o, &Shard { negatives: None, ..m });
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&v),
        "pre-image: the map shard has no negative inputs (run `slip parse` again)"
    );
}

// Item 3: the identity image is treated as O2 treats it: N1 does not
// apply (every change since the map phase is the edit compared). The
// drift is then seen by the comparison: not verified.
#[test]
fn n1_identity_image() {
    let (tree, o, _) = cfg_tree("n1-identity", CFG_PRE, CFG_PRE, &["ext2", "ext"]);
    write(&tree.base.join("ext2/cfg.h"), "#define USE_NEW 1\n");
    let params = Params { image: Image::identity(), ..tree.params(Declared::none()) };
    let v = tree.verify_with(&o, &params).unwrap();
    assert!(!reason(&v).contains("appeared"), "{}", reason(&v));
    assert_ne!(kind(&v), VerdictKind::Verified);
}

// Item 5: a file added in the includer's directory for a quoted include
// the map found on the list. The includer `wrap/w.h` is outside the roots
// and its directory is not on the list (`a.cpp` reaches it as
// `"../wrap/w.h"`), so only the includer's-directory rule records it. Its
// place, `s/../wrap/cfg.h`, is spelled through the subject root: N1 must
// decide "under a subject root" from the real path of the file now there,
// not from the spelling (which once verified this case wrongly).
#[test]
fn n1_quoted_include_beside_the_includer() {
    let pre = CFG_PRE.replace("<cfg.h>", "\"../wrap/w.h\"");
    let post = CFG_POST.replace("<cfg.h>", "\"../wrap/w.h\"");
    let (tree, o, _) = cfg_tree_with("n1-beside", &pre, &post, &["ext"], |t| {
        write(&t.base.join("wrap/w.h"), "#include \"cfg.h\"\n");
    });
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    write(&tree.base.join("wrap/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.subject.join("../wrap/cfg.h")));
}

const HAS_PRE: &str =
    "#if __has_include(<cfg2.h>)\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
const HAS_POST: &str =
    "#if !__has_include(<cfg2.h>)\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";

// Item 5: `__has_include` false at the map phase and true now, with an
// edit that compensates. The pair: mismatch.
#[test]
fn n1_has_include_now_true() {
    let (tree, o, _) = cfg_tree("n1-has", HAS_PRE, HAS_POST, &["ext"]);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    write(&tree.base.join("ext/cfg2.h"), "\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.base.join("ext/cfg2.h")));
}

// Item 5: `#include_next` with a file added between the includer's
// directory and the one the map found it in.
#[test]
fn n1_include_next_between() {
    let pre = CFG_PRE.replace("<cfg.h>", "<n.h>");
    let post = CFG_POST.replace("<cfg.h>", "<n.h>");
    let (tree, o, _) = cfg_tree_with("n1-next", &pre, &post, &["one", "two", "three"], |t| {
        write(&t.base.join("one/n.h"), "#include_next <n.h>\n");
        write(&t.base.join("three/n.h"), "#define USE_NEW 0\n");
        std::fs::create_dir_all(t.base.join("two")).unwrap();
    });
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    write(&tree.base.join("two/n.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.base.join("two/n.h")));
}

// Item 5: a search directory that did not exist at the map phase (Kiln
// left it out of the list) and now exists with the header.
#[test]
fn n1_directory_created_since() {
    let (tree, o, m) = cfg_tree("n1-created", CFG_PRE, CFG_POST, &["later", "ext"]);
    let negatives = m.negatives.as_ref().unwrap();
    assert!(negatives.contains(&bytes(&tree.base.join("later/cfg.h"))), "{negatives:?}");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    write(&tree.base.join("later/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.base.join("later/cfg.h")));
}

// Fix round, F2: a search directory missing at the map phase that comes
// after the directory the lookup found its file in. Created since with the
// header, it is not searched: no negative, and the verdict is the pair's.
#[test]
fn n1_directory_created_after_the_hit() {
    let (tree, o, m) = cfg_tree("n1-created-after", CFG_PRE, CFG_POST, &["ext", "gen"]);
    let negatives = m.negatives.as_ref().unwrap();
    assert!(!negatives.contains(&bytes(&tree.base.join("gen/cfg.h"))), "{negatives:?}");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    write(&tree.base.join("gen/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// ----- O2's second part (§5.3, WP-117 fix round F1): every input of the
// map shard under no subject root that the post-image parse did not read
// is still a regular file of its size and content.

fn left(p: &Path, how: &str) -> String {
    format!("pre-image: {} {how} since the map phase (run `slip parse` again)", p.display())
}

/// `-Iext-a -Iext`: the map reads `ext-a/cfg.h` (`USE_NEW 0`); `ext/cfg.h`
/// (`USE_NEW 1`) is there behind it.
fn found_first(name: &str) -> (Tree, Observer, Shard) {
    cfg_tree_with(name, CFG_PRE, CFG_POST, &["ext-a", "ext"], |t| {
        write(&t.base.join("ext-a/cfg.h"), "#define USE_NEW 0\n");
        write(&t.base.join("ext/cfg.h"), "#define USE_NEW 1\n");
    })
}

// F1 case A: the header the map read is deleted; the search now finds the
// one behind it, and the edit compensates. The negatives are empty: N1
// has nothing to check. Refused by O2's second part.
#[test]
fn o2_found_outside_header_deleted() {
    let (tree, o, m) = found_first("o2-deleted");
    assert_eq!(m.negatives.as_deref(), Some(&[][..]), "{:?}", m.negatives);
    let cfg = tree.base.join("ext-a/cfg.h");
    std::fs::remove_file(&cfg).unwrap();
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), left(&cfg, "is gone"));
}

#[test]
fn o2_found_outside_header_deleted_pair() {
    let (tree, o, _) = found_first("o2-deleted-pair");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// F1 case B: `__has_include` true at the map phase (the file is in the pre
// shard's `inputs`), the file deleted since, the edit compensating.
#[test]
fn o2_has_include_true_then_deleted() {
    let mk = |t: &Tree| write(&t.base.join("ext/cfg2.h"), "\n");
    let (tree, o, m) = cfg_tree_with("o2-has-gone", HAS_PRE, HAS_POST, &["ext"], mk);
    let cfg2 = tree.base.join("ext/cfg2.h");
    let inputs = m.inputs.as_ref().unwrap();
    assert!(inputs.iter().any(|f| f.path == bytes(&cfg2)), "{inputs:?}");
    std::fs::remove_file(&cfg2).unwrap();
    let v = tree.verify(&o);
    assert_eq!(reason(&v), left(&cfg2, "is gone"));
}

#[test]
fn o2_has_include_true_then_deleted_pair() {
    let mk = |t: &Tree| write(&t.base.join("ext/cfg2.h"), "\n");
    let (tree, o, _) = cfg_tree_with("o2-has-gone-pair", HAS_PRE, HAS_POST, &["ext"], mk);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

/// The pre `a.cpp` includes `<old.h>`, which the post `a.cpp` no longer
/// does: the post parse does not read `ext/old.h`.
const OLD_PRE: &str = "#include <old.h>\nint f() { return 1; }\n";
const OLD_POST: &str = "int f() { return 1; }\n";

fn dropped_include(name: &str) -> (Tree, Observer, Shard) {
    cfg_tree_with(name, OLD_PRE, OLD_POST, &["ext"], |t| {
        write(&t.base.join("ext/old.h"), "// v1\n");
    })
}

// F1: a file of the map parse's `inputs` that no post parse reads, whose
// bytes changed since (same size: the digest decides).
#[test]
fn o2_unread_input_changed() {
    let (tree, o, _) = dropped_include("o2-unread-changed");
    let old = tree.base.join("ext/old.h");
    write(&old, "// v2\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), left(&old, "changed"));
}

// F1: unchanged, it is not refused: the verdict is the comparison's.
#[test]
fn o2_unread_input_unchanged() {
    let (tree, o, _) = dropped_include("o2-unread-same");
    let v = tree.verify(&o);
    assert_ne!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
}

// Item 5: a file added and removed again before the job: nothing exists,
// nothing is refused; the edit is seen.
#[test]
fn n1_added_and_removed() {
    let (tree, o, _) = cfg_tree("n1-gone", CFG_PRE, CFG_POST, &["ext2", "ext"]);
    let cfg = tree.base.join("ext2/cfg.h");
    write(&cfg, "#define USE_NEW 1\n");
    std::fs::remove_file(&cfg).unwrap();
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// Item 5: a name with `..` components, through a directory missing at the
// map phase: the place keeps them (`ext/sub/../../ext2/cfg.h`), which
// exists only once `ext/sub` does, as for Kiln.
#[test]
fn n1_dot_dot_components() {
    let (tree, o, m) = cfg_tree("n1-dotdot", CFG_PRE, CFG_POST, &["ext/sub/../../ext2", "ext"]);
    let place = tree.base.join("ext/sub/../../ext2/cfg.h");
    assert!(m.negatives.as_ref().unwrap().contains(&bytes(&place)), "{:?}", m.negatives);
    write(&tree.base.join("ext2/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair, `ext/sub` missing: {}", reason(&v));
    std::fs::create_dir_all(tree.base.join("ext/sub")).unwrap();
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&place));
}

// Item 5: two spellings of one directory are one place.
#[test]
fn n1_two_spellings_of_one_directory() {
    let (tree, o, m) = cfg_tree("n1-spellings", CFG_PRE, CFG_POST, &["ext2", "ext2/./", "ext"]);
    let n = m.negatives.as_ref().unwrap();
    let place = bytes(&tree.base.join("ext2/cfg.h"));
    assert_eq!(n.iter().filter(|p| **p == place).count(), 1, "{n:?}");
    assert!(!n.iter().any(|p| p.ends_with(b"./cfg.h")), "{n:?}");
    write(&tree.base.join("ext2/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.base.join("ext2/cfg.h")));
}

// Item 5: a negative under a post root reached by a symbolic link: the
// place is `lnk/cfg.h` as written; it is not under a subject root
// (`Resolver::under_subject`: the region of its directory by identity, the
// nearest root, here the post root), so it is checked.
#[test]
fn n1_negative_under_a_post_root_by_a_link() {
    let (tree, o, _) = {
        let tree = Tree::new("n1-postlink", &[("a.cpp", CFG_PRE)]);
        write(&tree.post.join("a.cpp"), CFG_POST);
        write(&tree.base.join("ext/cfg.h"), "#define USE_NEW 0\n");
        std::fs::create_dir_all(tree.post.join("inc")).unwrap();
        link(tree.post.join("inc").to_str().unwrap(), &tree.base.join("lnk"));
        let flags = [arg("-I", &tree.base.join("lnk")), arg("-I", &tree.base.join("ext"))];
        let o = tree.observer_in(&tree.subject, &flags);
        let m = tree.map(&o);
        (tree, o, m)
    };
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    write(&tree.post.join("inc/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), appeared(&tree.base.join("lnk/cfg.h")));
}

// Item 5: a symbolic link outside the roots retargeted after the map
// phase: `lnk` led to `ext-a` (no `cfg.h`) and now leads to `ext-b`, which
// has one. The place is `lnk/cfg.h` as written, followed when N1 looks.
#[test]
fn n1_link_retargeted_since() {
    let tree = Tree::new("n1-retarget", &[("a.cpp", CFG_PRE)]);
    write(&tree.post.join("a.cpp"), CFG_POST);
    write(&tree.base.join("ext/cfg.h"), "#define USE_NEW 0\n");
    std::fs::create_dir_all(tree.base.join("ext-a")).unwrap();
    write(&tree.base.join("ext-b/cfg.h"), "#define USE_NEW 1\n");
    link("ext-a", &tree.base.join("lnk"));
    let flags = [arg("-I", &tree.base.join("lnk")), arg("-I", &tree.base.join("ext"))];
    let o = tree.observer_in(&tree.subject, &flags);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    std::fs::remove_file(tree.base.join("lnk")).unwrap();
    link("ext-b", &tree.base.join("lnk"));
    let v = tree.verify(&o);
    assert_eq!(reason(&v), appeared(&tree.base.join("lnk/cfg.h")));
}

// Beyond N1: a symbolic link outside the roots through which the map
// parse FOUND `cfg.h`, retargeted after the map phase to a directory with
// another `cfg.h`. The place was found, so it is no negative; O2 compares
// the files both parses read by real path, and the post parse reads
// `ext-b/cfg.h`, which the map parse did not. With an edit that
// compensates, the verdict is `verified` in a world the map phase never
// saw. Closing it needs the map shard to keep where each found lookup led
// (a positive counterpart of the negatives): WP-135.
#[test]
#[ignore = "needs: a record of where the map parse's found lookups led (WP-135)"]
fn o2_link_to_a_found_header_retargeted() {
    let tree = Tree::new("o2-retarget", &[("a.cpp", CFG_PRE)]);
    write(&tree.post.join("a.cpp"), CFG_POST);
    write(&tree.base.join("ext-a/cfg.h"), "#define USE_NEW 0\n");
    write(&tree.base.join("ext-b/cfg.h"), "#define USE_NEW 1\n");
    link("ext-a", &tree.base.join("lnk"));
    let o = tree.observer_in(&tree.subject, &[arg("-I", &tree.base.join("lnk"))]);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "pair: {}", reason(&v));
    std::fs::remove_file(tree.base.join("lnk")).unwrap();
    link("ext-b", &tree.base.join("lnk"));
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
}

// ----- G6 (§5.3): the links along the post parse's routes are the subject's

const G6_DEP: &str = "#pragma once\nnamespace old { struct D { int a; }; }\n";
const G6_MAIN: &str = "#include \"inc/d.h\"\nint f(old::D *d) { return d->a; }\n";

fn link(target: &str, at: &Path) {
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(target, at).unwrap();
}

/// A tree whose `a.cpp` includes `inc/d.h`; the subject's `inc` is made by
/// `subject`, the post tree's by `post`. The map shard is published.
fn g6_tree(name: &str, subject: impl Fn(&Path), post: impl Fn(&Path)) -> (Tree, Observer) {
    let tree = Tree::new(name, &[("a.cpp", G6_MAIN)]);
    subject(&tree.subject);
    post(&tree.post);
    let o = tree.observer();
    tree.map(&o);
    (tree, o)
}

fn dep_through_link(root: &Path) {
    write(&root.join("dep/d.h"), G6_DEP);
    link("dep", &root.join("inc"));
}

fn dep_as_directory(root: &Path) {
    write(&root.join("dep/d.h"), G6_DEP);
    write(&root.join("inc/d.h"), G6_DEP);
}

fn show(p: &Path) -> String {
    p.to_str().unwrap().to_owned()
}

// The review's probe (F1): `inc` a link in the subject, a real directory in
// the post tree (a copy that dereferenced links). The post file
// `post/inc/d.h` stands for `s/inc/d.h`, which is no real path of the
// subject: refused by G6 (ii), under rules and without.
#[test]
fn g6_link_in_subject_directory_in_post() {
    let (tree, o) = g6_tree("g6-probe", dep_through_link, dep_as_directory);
    let want = format!(
        "post-image: {} stands for {}, which the subject reaches through the link {}",
        show(&tree.post.join("inc/d.h")),
        show(&tree.subject.join("inc/d.h")),
        show(&tree.subject.join("inc"))
    );
    let v = tree.verify(&o);
    assert_eq!((kind(&v), reason(&v)), (VerdictKind::Unverifiable, want.as_str()));
    // The dependency's namespace relocated too, under a rule with `dep`
    // read-only: the shard that said `verified` before G6.
    write(&tree.post.join("inc/d.h"), &G6_DEP.replace("old", "fresh"));
    write(&tree.post.join("a.cpp"), &G6_MAIN.replace("old::", "fresh::"));
    let declared = Declared::new(vec![rename_ns("fresh", "old")], vec![], vec![], vec![])
        .unwrap()
        .with_files(Some(tree.files(&["dep"])));
    let v = tree.verify_with(&o, &tree.params(declared)).unwrap();
    assert_eq!((kind(&v), reason(&v)), (VerdictKind::Unverifiable, want.as_str()));
}

// The mirror: a real directory in the subject, a link in the post tree:
// the post parse follows `post/inc`, which is a directory in the subject.
#[test]
fn g6_directory_in_subject_link_in_post() {
    let (tree, o) = g6_tree("g6-mirror", dep_as_directory, dep_through_link);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    let r = reason(&v);
    assert!(r.starts_with("post-image: "), "{r}");
    assert!(
        r.ends_with(&format!(
            " follows the link {}, which is a directory in the subject",
            show(&tree.post.join("inc"))
        )),
        "{r}"
    );
}

// One link in both trees, with one target: verified (no refusal).
#[test]
fn g6_same_link_in_both() {
    let (tree, o) = g6_tree("g6-same", dep_through_link, dep_through_link);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
}

// One link name, another target.
#[test]
fn g6_same_link_other_target() {
    let (tree, o) = g6_tree("g6-target", dep_through_link, |p: &Path| {
        write(&p.join("dep2/d.h"), G6_DEP);
        link("dep2", &p.join("inc"));
    });
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    let r = reason(&v);
    assert!(
        r.ends_with(&format!(
            " follows the link {}, which is a link to dep in the subject",
            show(&tree.post.join("inc"))
        )),
        "{r}"
    );
}

// A link the post tree adds where the subject has nothing: the subject
// includes `dep/d.h`, the post tree `inc/d.h` through a new link.
#[test]
fn g6_link_only_in_post() {
    let tree = Tree::new("g6-added", &[("a.cpp", &G6_MAIN.replace("inc/", "dep/"))]);
    write(&tree.subject.join("dep/d.h"), G6_DEP);
    dep_through_link(&tree.post);
    write(&tree.post.join("a.cpp"), G6_MAIN);
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    let r = reason(&v);
    assert!(
        r.ends_with(&format!(
            " follows the link {}, which is missing in the subject",
            show(&tree.post.join("inc"))
        )),
        "{r}"
    );
}

// A link at the file itself: `inc/d.h -> ../dep/d.h`. The same link in
// both trees verifies; the post tree's real file at `inc/d.h` stands for a
// subject path that is the link: refused by (ii).
#[test]
fn g6_file_level_link() {
    let file_link = |root: &Path| {
        write(&root.join("dep/d.h"), G6_DEP);
        link("../dep/d.h", &root.join("inc/d.h"));
    };
    let (tree, o) = g6_tree("g6-file", file_link, file_link);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
    // One `Tree` at a time: each holds the tests' lock.
    drop(tree);
    let (tree, o) = g6_tree("g6-file-copy", file_link, dep_as_directory);
    let v = tree.verify(&o);
    assert_eq!(
        reason(&v),
        format!(
            "post-image: {} stands for {}, which the subject reaches through the link {}",
            show(&tree.post.join("inc/d.h")),
            show(&tree.subject.join("inc/d.h")),
            show(&tree.subject.join("inc/d.h"))
        )
    );
}

// A link on the main file's route: the observer compiles `s/src/a.cpp`,
// `src` a link to `real` in the subject and a real directory in the post
// tree.
#[test]
fn g6_link_on_the_main_route() {
    let main = "int f() { return 1; }\n";
    let tree = Tree::new("g6-main", &[]);
    write(&tree.subject.join("real/a.cpp"), main);
    link("real", &tree.subject.join("src"));
    write(&tree.post.join("real/a.cpp"), main);
    write(&tree.post.join("src/a.cpp"), main);
    let o = tree.observer_of("src/a.cpp", &[]);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    let r = reason(&v);
    assert!(r.contains(&show(&tree.subject.join("src"))), "{r}");
}

// A link outside both roots (an include directory reached through a link,
// as system and build trees have them) is not G6's: the post parse follows
// it as the pre parse did, and the image verifies.
#[test]
fn g6_link_outside_the_roots() {
    let tree = Tree::new("g6-outside", &[("a.cpp", "#include <e.h>\nint f() { return E; }\n")]);
    write(&tree.base.join("ext-real/e.h"), "#define E 1\n");
    link("ext-real", &tree.base.join("ext"));
    let o = tree.observer_of("a.cpp", &[&arg("-I", &tree.base.join("ext"))]);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
}

// A created file in a created directory: no refusal (its subject path does
// not exist, under real directories).
#[test]
fn g6_created_file_in_a_created_directory() {
    let tree = Tree::new("g6-created", &[("a.cpp", "int f() { return 1; }\n")]);
    write(&tree.post.join("new/dir/n.h"), "#pragma once\nint n();\n");
    write(&tree.post.join("a.cpp"), "#include \"new/dir/n.h\"\nint f() { return 1; }\n");
    let o = tree.observer();
    tree.map(&o);
    let v = tree.verify(&o);
    assert_ne!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
}

// WP-121 with G6: a G6 `unverifiable` shard under rules carries `binding`
// and `rule_use`, as G1's does (`g1_under_rules_keeps_the_sections`).
#[test]
fn g6_under_rules_keeps_the_sections() {
    let (tree, o) = g6_tree("g6-rules", dep_through_link, dep_as_directory);
    let params = tree.params(declared_of(&tree, rename_ns("fresh", "old"), &["dep"]));
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert!(reason(&v).contains(" which the subject reaches through the link "), "{}", reason(&v));
    check_sections_under_rules(&v, &params);
}

// ----- G6 (iii) (§5.3): a route that leaves a post root ends where the
// subject's does

/// The reviewer's layout (G6-F1, G6-F2): the subject `a/s`, the post tree
/// `b/post`, each with an outside directory `ext` beside it whose `k.h`
/// defines another macro; `a.cpp` is `main`.
fn g6_apart(name: &str, main: &str, flags: &[&str]) -> (Tree, Observer) {
    let tree = Tree::new_in(name, "a/s", "b/post", &[("a.cpp", main)]);
    write(&tree.base.join("a/ext/k.h"), "#pragma once\n#define K 1\n");
    write(&tree.base.join("b/ext/k.h"), "#pragma once\n#define K 1\n#define K2 1\n");
    let o = tree.observer_of("a.cpp", flags);
    (tree, o)
}

fn leaves_at(tree: &Tree, what: &str, here: &str, there: &str) -> String {
    format!(
        "post-image: {what}, which leaves the post root: {} here, {} from the subject",
        show(&tree.base.join(here)),
        show(&tree.base.join(there))
    )
}

// G6-F1: `inc -> ../ext` in both trees, the same bytes, two places.
#[test]
fn g6_relative_link_leaving_the_root() {
    let (tree, o) = g6_apart("g6-leave-link", "#include \"inc/k.h\"\nint f() { return K; }\n", &[]);
    link("../ext", &tree.subject.join("inc"));
    link("../ext", &tree.post.join("inc"));
    tree.map(&o);
    write(&tree.post.join("a.cpp"), "#include \"inc/k.h\"\nint f() { return K2; }\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    let what = format!("looks for inc/k.h in {}", show(&tree.post));
    assert_eq!(reason(&v), leaves_at(&tree, &what, "b/ext/k.h", "a/ext/k.h"));
}

// G6-F2: the same without a link: `#include "../ext/k.h"`.
#[test]
fn g6_relative_include_leaving_the_root() {
    let (tree, o) =
        g6_apart("g6-leave-inc", "#include \"../ext/k.h\"\nint f() { return K; }\n", &[]);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    let what = format!("looks for ../ext/k.h in {}", show(&tree.post));
    assert_eq!(reason(&v), leaves_at(&tree, &what, "b/ext/k.h", "a/ext/k.h"));
}

// The sibling layout: `s` and `post-tree` side by side reach one `ext` by
// `../ext`, through a link and by an `#include`: one real path, verified.
#[test]
fn g6_sibling_reaches_the_same_outside() {
    let main = "#include \"inc/k.h\"\n#include \"../ext/j.h\"\nint f() { return K + J; }\n";
    let tree = Tree::new("g6-sibling", &[("a.cpp", main)]);
    write(&tree.base.join("ext/k.h"), "#pragma once\n#define K 1\n");
    write(&tree.base.join("ext/j.h"), "#pragma once\n#define J 1\n");
    link("../ext", &tree.subject.join("inc"));
    link("../ext", &tree.post.join("inc"));
    let o = tree.observer_of("a.cpp", &["-I../ext"]);
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
}

// A location tried and missing, which exists beside the subject only
// (`__has_include` reads nothing), then beside the post tree only: the
// place differs either way.
#[test]
fn g6_tried_location_beside_one_tree() {
    let main = "#if __has_include(\"../opt/o.h\")\nint f() { return 1; }\n#else\nint f() { return 2; }\n#endif\n";
    for (n, beside) in [(0, "a/opt/o.h"), (1, "b/opt/o.h")] {
        let (tree, o) = g6_apart(&format!("g6-tried-{n}"), main, &[]);
        write(&tree.base.join(beside), "#define O 1\n");
        tree.map(&o);
        let v = tree.verify(&o);
        assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
        let what = format!("looks for ../opt/o.h in {}", show(&tree.post));
        assert_eq!(reason(&v), leaves_at(&tree, &what, "b/opt/o.h", "a/opt/o.h"));
    }
}

// A directory of the search list that leaves the root through a link
// (`-Iinc`, `inc -> ../ext`), which no lookup reaches: `k.h` is found in
// `.` first. (A word of the command that leaves by `..` is rewritten to
// the subject's place, §5.1, and does not leave.)
#[test]
fn g6_search_directory_leaving_the_root() {
    let main = "#include <k.h>\nint f() { return K; }\n";
    let (tree, o) = g6_apart("g6-list", main, &["-I.", "-Iinc"]);
    for root in [&tree.subject, &tree.post] {
        write(&root.join("k.h"), "#define K 1\n");
        link("../ext", &root.join("inc"));
    }
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), leaves_at(&tree, "searches inc", "b/ext", "a/ext"));
}

// A link chain that leaves the root and comes back in: `inc -> ../ext/back`
// and `ext/back -> ../<root>/real` beside each tree. Back into its own
// tree on each side: verified. Back into the post tree's name from the
// subject's side (where no such directory is): refused.
#[test]
fn g6_link_chain_leaves_and_comes_back() {
    for (n, back) in [(0, None), (1, Some("../post/real"))] {
        let main = "#include \"inc/k.h\"\nint f() { return K; }\n";
        let (tree, o) = g6_apart(&format!("g6-back-{n}"), main, &[]);
        for (root, name) in [(&tree.subject, "s"), (&tree.post, "post")] {
            write(&root.join("real/k.h"), "#pragma once\n#define K 1\n");
            link("../ext/back", &root.join("inc"));
            let outside = root.parent().unwrap().join("ext/back");
            link(back.unwrap_or(&format!("../{name}/real")), &outside);
        }
        // What the subject's chain reaches when it names the post tree.
        write(&tree.base.join("a/post/real/k.h"), "#pragma once\n#define K 1\n");
        tree.map(&o);
        let v = tree.verify(&o);
        if back.is_none() {
            assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
        } else {
            assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
            let what = format!("looks for inc/k.h in {}", show(&tree.post));
            assert_eq!(reason(&v), leaves_at(&tree, &what, "b/post/real/k.h", "a/post/real/k.h"));
        }
    }
}

// A header in a subdirectory leaves through `..`: `sub/h.h` includes
// `../../ext/k.h`.
#[test]
fn g6_header_leaves_through_dot_dot() {
    let main = "#include \"sub/h.h\"\nint f() { return K; }\n";
    let (tree, o) = g6_apart("g6-sub", main, &[]);
    write(&tree.subject.join("sub/h.h"), "#include \"../../ext/k.h\"\n");
    write(&tree.post.join("sub/h.h"), "#include \"../../ext/k.h\"\n");
    tree.map(&o);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    let r = reason(&v);
    assert!(r.contains("../../ext/k.h"), "{r}");
    assert!(
        r.ends_with(&format!(
            " which leaves the post root: {} here, {} from the subject",
            show(&tree.base.join("b/ext/k.h")),
            show(&tree.base.join("a/ext/k.h"))
        )),
        "{r}"
    );
}

// Under rules, a G6 (iii) refusal keeps `binding` and `rule_use`.
#[test]
fn g6_leaving_under_rules_keeps_the_sections() {
    let main = "#include \"../ext/k.h\"\nnamespace old { int f() { return K; } }\n";
    let (tree, o) = g6_apart("g6-leave-rules", main, &[]);
    tree.map(&o);
    write(&tree.post.join("a.cpp"), &main.replace("old", "fresh"));
    let params = tree.params(declared_of(&tree, rename_ns("fresh", "old"), &[]));
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(kind(&v), VerdictKind::Unverifiable);
    assert!(reason(&v).contains(", which leaves the post root: "), "{}", reason(&v));
    check_sections_under_rules(&v, &params);
}

// ---------------------------------------------------------------------------
// WP-121: `binding` and `rule_use` under rules (spec/verify.md §9.1, §5.4)

/// A golden of `crates/slip_cli/tests/golden/`; `SLIP_BLESS=1` rewrites it.
fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

/// The JSON rendering of a verify shard with what does not depend on this
/// crate taken out: the tree's directory is `<base>`, the map shard's
/// content id `<pre>`, the observer's id (a hash of a command naming
/// `<base>`) `<observer>`, the counters those of §7.2 only (the parse's and
/// the walk's are Kiln's and `slip_digest`'s), and no `digests` (the
/// walk's). Every section the shard has is in it, by name. A finding's
/// key and unit digests are the walk's too: a change of the digest scheme
/// moves them, and is re-blessed with it.
fn shard_view(tree: &Tree, v: &Shard, pre: Option<Digest>) -> String {
    let mut s = v.clone();
    s.counters.retain(|c| c.name.starts_with("verify."));
    s.digests = None;
    s.names = None;
    let mut text = slip_model::json::write_pretty(&s.to_json()).unwrap();
    text = text.replace(tree.base.to_str().unwrap(), "<base>");
    text = text.replace(&v.observer.to_string(), "<observer>");
    if let Some(pre) = pre {
        text = text.replace(&pre.to_string(), "<pre>");
    }
    text
}

// §5.4, §9.1 (WP-121 item 3): a job whose rule table is empty writes the
// shard it wrote before WP-121, byte for byte: neither `binding` nor
// `rule_use`, for each outcome. The golden was blessed on `main` before
// the package's code changed (spec/landed/verify/WP-121.md §3).
#[test]
fn s9_1_no_rules_shard_unchanged() {
    const PRE: &str = "int f() { return 1; }\nint g() { return f(); }\n";
    let cases: [(&str, Option<&str>, bool); 4] = [
        ("verified", None, true),
        ("mismatch", Some("int f() { return 2; }\nint g() { return f(); }\n"), true),
        ("diagnosed", Some("int f( { return 1; }\n"), true),
        ("unverifiable", None, false),
    ];
    let mut out = String::new();
    for (name, post, map) in cases {
        let tree = Tree::new(&format!("norules-{name}"), &[("a.cpp", PRE)]);
        if let Some(post) = post {
            write(&tree.post.join("a.cpp"), post);
        }
        let o = tree.observer();
        let pre = map.then(|| {
            tree.map(&o);
            tree.stored_map(&o).1
        });
        // `Declared::none()` (no `--declared`) and one with an exemption
        // and no rule (files need a rule, `spec/verify.md` §2).
        let exempting = Declared::new(vec![], vec![Digest([1; 32])], vec![], vec![]).unwrap();
        for declared in [Declared::none(), exempting] {
            let v = tree.verify_with(&o, &tree.params(declared)).unwrap();
            assert!(v.binding.is_none() && v.rule_use.is_none(), "{name}");
        }
        let v = tree.verify(&o);
        out.push_str(name);
        out.push('\n');
        out.push_str(&shard_view(&tree, &v, pre));
        out.push('\n');
    }
    golden("verify-no-rules.txt", &out);
}

/// The declared changes of one rule, the subject owned, with
/// `readonly` files of it read-only.
fn declared_of(tree: &Tree, rule: Rule, readonly: &[&str]) -> Declared {
    Declared::new(vec![rule], vec![], vec![], vec![])
        .unwrap()
        .with_files(Some(tree.files(readonly)))
}

const RELOCATE_PRE: &str =
    "namespace old { int f() { return 1; } }\nint g() { return old::f(); }\n";
const RELOCATE_POST: &str =
    "namespace fresh { int f() { return 1; } }\nint g() { return fresh::f(); }\n";

// §9.1, §5.4 (WP-121 items 1 and 2): under a rule table that is not
// empty, every shard the job writes has `binding` (no overlay, the
// declared changes' content id, the pre round), whatever its outcome; an
// `ok` one has `rule_use` (the rules' id, the walk's counts), a
// `diagnosed` one has none; a job that fails writes no shard (the
// coordinator's `failed` shard has no binding).
#[test]
fn s9_1_sections_per_outcome() {
    let rule = rename_ns("fresh", "old");
    let relocated = |tree: &Tree| declared_of(tree, rule.clone(), &[]);
    let used = vec![slip_model::agreement::Use { rule: 0, via: 1, direct: 0, kept_via: 0 }];
    let check = |tree: &Tree, v: &Shard, params: &Params, rule_use: bool| {
        let id = content_id(&params.declared.to_bytes()).unwrap();
        let want = slip_model::Binding { overlay: None, declared: id, pre: params.pre };
        assert_eq!(v.binding, Some(want), "{:?}", v.outcome);
        match &v.rule_use {
            Some(u) if rule_use => {
                assert_eq!(u.rules, params.declared.rules_id());
                assert_eq!(u.uses, used, "{}", tree.base.display());
            }
            None if !rule_use => {}
            other => panic!("rule_use {other:?} in {:?}", v.outcome),
        }
    };

    // `ok`, verified.
    let tree = Tree::new("sec-verified", &[("a.cpp", RELOCATE_PRE)]);
    write(&tree.post.join("a.cpp"), RELOCATE_POST);
    let o = tree.observer();
    tree.map(&o);
    let params = tree.params(relocated(&tree));
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
    check(&tree, &v, &params, true);
    // `ok`, unverifiable by a guard (P1: no map shard of round 5); the
    // binding names that round.
    let params5 = Params { pre: 5, ..params.clone() };
    let v = tree.verify_with(&o, &params5).unwrap();
    assert_eq!(reason(&v), "pre-image: no map shard");
    check(&tree, &v, &params5, true);
    // A job that fails (the map shard altered): no shard at all.
    let path = tree.store.shard_path(&Digest([7; 32]), Phase::Map, 0, &o.id);
    let mut stored = std::fs::read(&path).unwrap();
    let last = stored.len() - 40;
    stored[last] ^= 1;
    std::fs::write(&path, &stored).unwrap();
    assert!(tree.verify_with(&o, &params).is_err());
    drop(tree);

    // `ok`, mismatch: the relocation and a changed body.
    let tree = Tree::new("sec-mismatch", &[("a.cpp", RELOCATE_PRE)]);
    write(&tree.post.join("a.cpp"), &RELOCATE_POST.replace("return 1", "return 2"));
    let o = tree.observer();
    tree.map(&o);
    let params = tree.params(relocated(&tree));
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(kind(&v), VerdictKind::Mismatch);
    check(&tree, &v, &params, true);
    drop(tree);

    // `diagnosed`: binding, no rule_use, no verdict.
    let tree = Tree::new("sec-diagnosed", &[("a.cpp", RELOCATE_PRE)]);
    write(&tree.post.join("a.cpp"), "namespace fresh { int f( { return 1; } }\n");
    let o = tree.observer();
    tree.map(&o);
    let params = tree.params(relocated(&tree));
    let v = tree.verify_with(&o, &params).unwrap();
    assert_eq!(v.outcome.status, Status::Diagnosed);
    assert!(v.verdict.is_none());
    check(&tree, &v, &params, false);
}

/// The exit status of `slip verify` for a run's verdict
/// (`spec/exec.md` §8.7).
fn exit_status(v: slip_verify::run::RunVerdict) -> u8 {
    match v {
        slip_verify::run::RunVerdict::Verified => 0,
        slip_verify::run::RunVerdict::NotVerified => 1,
        slip_verify::run::RunVerdict::Incomplete => 4,
    }
}

/// The run's report over the verify shards of `observers` (round 1, pre
/// round 0), each added with its map shard's status, as `slip verify`
/// adds them (`spec/exec.md` §8.7): what `slip verify` prints, short of
/// the executable (WP-053).
fn run_report(
    tree: &Tree,
    params: &Params,
    shards: &[(Observer, Shard)],
) -> slip_verify::run::Report {
    use slip_verify::run::{Header, Report};
    let header = Header {
        plan: Digest([7; 32]),
        round: 1,
        declared: params.declared_id,
        rules: params.declared.rules_id(),
        image: vec![(bytes(&tree.subject), bytes(&tree.post))],
        limit: params.limit as u64,
        locate: params.locate as u64,
        plan_observers: shards.len() as u64,
        run: b"runs/0001.run".to_vec(),
    };
    let mut r = Report::new(header, &params.declared);
    for (o, v) in shards {
        let (map, _) = tree.stored_map(o);
        r.add(&o.id, &o.main.path, Some(v), Some(map.outcome.status));
    }
    r
}

const RELOCATE_HEADER_PRE: &str =
    "namespace old { struct S { int x; }; inline int f(S s) { return s.x; } }\n";
const RELOCATE_U0: &str = "#include \"a.h\"\nint u0() { old::S s{1}; return f(s); }\n";
const RELOCATE_U1: &str = "#include \"a.h\"\nint u1() { return old::f(old::S{2}); }\n";

/// A relocation of `old` to `fresh` over two observers sharing `a.h`;
/// with `wrong`, `u1`'s edit also changes a value. Each observer's verify
/// shard, under the rule with `readonly` files of the subject read-only.
fn relocation(
    name: &str,
    wrong: bool,
    readonly: &[&str],
) -> (Tree, Params, Vec<(Observer, Shard)>) {
    let tree = Tree::new(
        name,
        &[("a.h", RELOCATE_HEADER_PRE), ("u0.cpp", RELOCATE_U0), ("u1.cpp", RELOCATE_U1)],
    );
    let fresh = |s: &str| s.replace("old", "fresh");
    write(&tree.post.join("a.h"), &fresh(RELOCATE_HEADER_PRE));
    write(&tree.post.join("u0.cpp"), &fresh(RELOCATE_U0));
    let u1 = fresh(RELOCATE_U1);
    write(&tree.post.join("u1.cpp"), &if wrong { u1.replace("S{2}", "S{3}") } else { u1 });
    let params = tree.params(declared_of(&tree, rename_ns("fresh", "old"), readonly));
    let mut shards = Vec::new();
    for main in ["u0.cpp", "u1.cpp"] {
        let o = tree.observer_of(main, &[]);
        tree.map(&o);
        let v = tree.verify_with(&o, &params).unwrap();
        shards.push((o, v));
    }
    (tree, params, shards)
}

// WP-121 item 5, §9.2, §9.3, `spec/exec.md` §8.7: a namespace relocation
// over two observers is `verified` in each, and the run is verified
// (status 0). WP-111: no observer is stale for the rule (nothing owned is
// left in `old`, nothing is kept: `direct` and `kept_via` 0), and no line
// says it is not checked (until WP-111 the rule was listed `unchecked`,
// which did not change the verdict). Before WP-121 both were `failed` (no
// `binding`, no `rule_use`) and the run incomplete (status 4). The wrong
// edit beside it: `mismatch`, not verified (status 1).
#[test]
fn s9_3_relocation_verified_over_two_observers() {
    use slip_verify::run::{RunVerdict, State};
    let (tree, params, shards) = relocation("reloc-right", false, &[]);
    for (o, v) in &shards {
        assert_eq!(kind(v), VerdictKind::Verified, "{}: {}", o.main.path.escape_ascii(), reason(v));
        let u = v.rule_use.as_ref().unwrap();
        assert!(u.uses.iter().all(|u| u.via > 0 && u.direct == 0 && u.kept_via == 0));
        assert!(u.kept.is_empty());
    }
    let r = run_report(&tree, &params, &shards);
    assert_eq!(r.states()[0], 2, "{}", r.error_lines());
    assert_eq!(r.verdict(), RunVerdict::Verified);
    assert_eq!(exit_status(r.verdict()), 0);
    assert_eq!((r.stale(), r.unused()), (&[][..], vec![]));
    let text = r.to_text();
    assert!(text.contains("agreement rules 1 stale 0 unused 0\n"), "{text}");
    assert!(!text.contains("not checked"), "{text}");
    assert_eq!(r.verdict_line(), "verdict verified");
    assert_eq!(r.error_lines(), "");
    drop(tree);

    let (tree, params, shards) = relocation("reloc-wrong", true, &[]);
    assert_eq!(kind(&shards[0].1), VerdictKind::Verified);
    assert_eq!(kind(&shards[1].1), VerdictKind::Mismatch);
    let r = run_report(&tree, &params, &shards);
    let at = |s: State| r.states()[State::ALL.iter().position(|&x| x == s).unwrap()];
    assert_eq!((at(State::Verified), at(State::Mismatch)), (1, 1));
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
    assert_eq!(exit_status(r.verdict()), 1);
}

// WP-121 item 5, §2.1, §9.2: the job-level counterpart of WP-053's
// `s2_1_rule_naming_a_dependency_entity_matches_nothing`. The namespace
// is declared in a read-only file: no rule reaches a dependency entity,
// so the relocation is a real change in each observer (`mismatch`), the
// rule matched nothing (`unused`), and the run is not verified (status
// 1). Before WP-121 the observers were `failed` and the run incomplete
// (status 4) for want of `rule_use`, which hid the mismatches.
#[test]
fn s2_1_rule_on_a_dependency_entity_not_verified() {
    use slip_verify::run::{RunVerdict, State};
    let (tree, params, shards) = relocation("reloc-dep", false, &["a.h"]);
    for (_, v) in &shards {
        assert_eq!(kind(v), VerdictKind::Mismatch, "{}", reason(v));
    }
    let r = run_report(&tree, &params, &shards);
    let at = |s: State| r.states()[State::ALL.iter().position(|&x| x == s).unwrap()];
    assert_eq!((at(State::Mismatch), at(State::Failed)), (2, 0));
    assert_eq!(r.unused(), [0]);
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
    assert_eq!(exit_status(r.verdict()), 1);
}

// WP-140 row (e), §2.1, §9.2: a relocation of `old` into `fresh`, which a
// read-only header also opens, over two observers: `u0` includes that
// header itself, `u1` sees its opening of `fresh` only through another
// read-only header. `fresh` has a unit in the owned view (keyed `old`, the
// namespace's one digest), so each observer is `verified`, with the same
// counts (`via` 1: the owned view's key matched the rule), and the run is
// verified. Until WP-140 each was `mismatch` (`removed old`, `via` 0).
#[test]
fn wp140_relocation_into_a_namespace_a_dependency_opens_over_two_observers() {
    use slip_verify::run::RunVerdict;
    let u0 = "#include \"dep/d.h\"\nnamespace old { inline int f0() { return 0; } }\nint u0() { return old::f0(); }\n";
    let u1 = "#include \"dep/w.h\"\nnamespace old { inline int f1() { return 1; } }\nint u1() { return old::f1(); }\n";
    let tree = Tree::new(
        "wp140-two-observers",
        &[
            ("dep/d.h", "namespace fresh { struct D {}; }\n"),
            ("dep/w.h", "#include \"d.h\"\n"),
            ("u0.cpp", u0),
            ("u1.cpp", u1),
        ],
    );
    write(&tree.post.join("u0.cpp"), &u0.replace("old", "fresh"));
    write(&tree.post.join("u1.cpp"), &u1.replace("old", "fresh"));
    let params = tree.params(declared_of(&tree, rename_ns("fresh", "old"), &["dep"]));
    let mut shards = Vec::new();
    for main in ["u0.cpp", "u1.cpp"] {
        let o = tree.observer_of(main, &[]);
        tree.map(&o);
        let v = tree.verify_with(&o, &params).unwrap();
        assert_eq!(kind(&v), VerdictKind::Verified, "{main}: {}", reason(&v));
        let u = v.rule_use.as_ref().unwrap();
        assert_eq!(
            u.uses,
            [slip_model::agreement::Use { rule: 0, via: 1, direct: 0, kept_via: 0 }]
        );
        shards.push((o, v));
    }
    let r = run_report(&tree, &params, &shards);
    assert_eq!(r.verdict(), RunVerdict::Verified, "{}", r.error_lines());
    assert_eq!((r.stale(), r.unused()), (&[][..], vec![]));
}

// WP-140 review F3: observers the edit did not touch that open the
// dependency's `fresh` only to specialise its template, or with an empty
// opening, hold nothing owned there: no owned view, so each is verified
// beside the observer that moves `old` (before the fix, `added fresh`).
#[test]
fn wp140_untouched_observers_that_open_the_dependency_namespace() {
    use slip_verify::run::RunVerdict;
    let u0 = "#include \"dep/d.h\"\nnamespace old { inline int f0() { return 0; } }\nint u0() { return old::f0(); }\n";
    let u1 = "#include \"dep/d.h\"\nnamespace fresh { template <> struct Trait<int> { static const int v = 2; }; }\nint u1() { return fresh::Trait<int>::v; }\n";
    let u2 = "#include \"dep/d.h\"\nnamespace fresh { }\nint u2() { return sizeof(fresh::D); }\n";
    let tree = Tree::new(
        "wp140-untouched",
        &[
            (
                "dep/d.h",
                "namespace fresh { struct D {}; template <class T> struct Trait { static const int v = 1; }; }\n",
            ),
            ("u0.cpp", u0),
            ("u1.cpp", u1),
            ("u2.cpp", u2),
        ],
    );
    write(&tree.post.join("u0.cpp"), &u0.replace("old", "fresh"));
    let params = tree.params(declared_of(&tree, rename_ns("fresh", "old"), &["dep"]));
    let mut shards = Vec::new();
    for (main, via) in [("u0.cpp", 1), ("u1.cpp", 0), ("u2.cpp", 0)] {
        let o = tree.observer_of(main, &[]);
        tree.map(&o);
        let v = tree.verify_with(&o, &params).unwrap();
        assert_eq!(kind(&v), VerdictKind::Verified, "{main}: {}", reason(&v));
        // A rule an observer did not use has no `Use` there.
        let want: Vec<_> = (via > 0)
            .then_some(slip_model::agreement::Use { rule: 0, via, direct: 0, kept_via: 0 })
            .into_iter()
            .collect();
        assert_eq!(v.rule_use.as_ref().unwrap().uses, want, "{main}");
        shards.push((o, v));
    }
    let r = run_report(&tree, &params, &shards);
    assert_eq!(r.verdict(), RunVerdict::Verified, "{}", r.error_lines());
    assert_eq!((r.stale(), r.unused()), (&[][..], vec![]));
}

// WP-111 review, mutant R5: the job walks with `Declared.kept` (§9.2). An
// owned member left in `old` counts `direct` unless its key is kept; kept,
// it counts none and its key is seen. A job that passed no kept key would
// count `direct` 1 in both, and the kept run would be stale.
#[test]
fn s9_2_the_job_walks_with_kept() {
    use slip_model::keys::Key;
    let pre =
        "namespace old { int f() { return 1; } int g; }\nint u0() { return old::f() + old::g; }\n";
    let tree = Tree::new("kept-job", &[("u0.cpp", pre)]);
    write(
        &tree.post.join("u0.cpp"),
        "namespace fresh { int f() { return 1; } }\nnamespace old { int g; }\nint u0() { return fresh::f() + old::g; }\n",
    );
    let old =
        Key { parent: Parent::Root, kind: KeyKind::Namespace, name: ident("old"), disc: None };
    let g = Key {
        parent: Parent::Unit(old.id()),
        kind: KeyKind::Variable,
        name: ident("g"),
        disc: None,
    };
    for kept in [false, true] {
        let mut declared = declared_of(&tree, rename_ns("fresh", "old"), &[]);
        if kept {
            declared = declared.with_kept(vec![g.id()]).unwrap();
        }
        let params = tree.params(declared);
        let o = tree.observer_of("u0.cpp", &[]);
        tree.map(&o);
        let v = tree.verify_with(&o, &params).unwrap();
        assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
        let u = v.rule_use.as_ref().unwrap();
        let want =
            slip_model::agreement::Use { rule: 0, via: 1, direct: u64::from(!kept), kept_via: 0 };
        assert_eq!(u.uses, [want], "kept {kept}");
        assert_eq!(u.kept, if kept { vec![0] } else { vec![] }, "kept {kept}");
    }
}

/// The rule renaming the class `from_post` at the root of the post-image
/// to `to_pre` of the pre-image.
fn rename_class(from_post: &str, to_pre: &str) -> Rule {
    Rule { kind: KeyKind::Class, ..rename_ns(from_post, to_pre) }
}

// WP-121 item 5, §9.2: a rename (a rule of kind `class`) over two
// observers, one of which still declares the old name itself and was not
// edited: each observer is `verified`, the second is stale for the rule
// (its `direct` is 1), and the run is not verified (status 1). The wrong
// edit beside it: `mismatch`.
#[test]
fn s9_2_rename_with_a_stale_observer() {
    use slip_verify::run::{RunVerdict, Stale, State};
    for wrong in [false, true] {
        let u0_pre = "#include \"a.h\"\nint u0() { S s{1}; return s.x; }\n";
        let u1 = "struct S { int y; };\nint u1() { S s{2}; return s.y; }\n";
        let tree = Tree::new(
            &format!("rename-{wrong}"),
            &[("a.h", "struct S { int x; };\n"), ("u0.cpp", u0_pre), ("u1.cpp", u1)],
        );
        write(&tree.post.join("a.h"), "struct T { int x; };\n");
        let u0_post = u0_pre.replace("S s", "T s");
        write(
            &tree.post.join("u0.cpp"),
            &if wrong { u0_post.replace("{1}", "{3}") } else { u0_post },
        );
        let params = tree.params(declared_of(&tree, rename_class("T", "S"), &[]));
        let mut shards = Vec::new();
        for main in ["u0.cpp", "u1.cpp"] {
            let o = tree.observer_of(main, &[]);
            tree.map(&o);
            shards.push((o.clone(), tree.verify_with(&o, &params).unwrap()));
        }
        let uses = |i: usize| shards[i].1.rule_use.as_ref().unwrap().uses.clone();
        let use_of = |via, direct| slip_model::agreement::Use { rule: 0, via, direct, kept_via: 0 };
        assert_eq!(uses(0), [use_of(1, 0)]);
        assert_eq!(uses(1), [use_of(0, 1)]);
        let want0 = if wrong { VerdictKind::Mismatch } else { VerdictKind::Verified };
        assert_eq!(kind(&shards[0].1), want0, "{}", reason(&shards[0].1));
        assert_eq!(kind(&shards[1].1), VerdictKind::Verified, "{}", reason(&shards[1].1));
        let r = run_report(&tree, &params, &shards);
        let at = |s: State| r.states()[State::ALL.iter().position(|&x| x == s).unwrap()];
        assert_eq!(at(State::Mismatch), u64::from(wrong));
        assert_eq!(
            r.stale(),
            [Stale { rule: 0, observer: shards[1].0.id, direct: 1, kept_via: 0 }]
        );
        assert!(r.unused().is_empty());
        assert!(r.to_text().contains("agreement rules 1 stale 1 unused 0\n"));
        assert_eq!(r.verdict(), RunVerdict::NotVerified);
        assert_eq!(exit_status(r.verdict()), 1);
    }
}

// ----- O2's second part, re-check round of WP-117: a recorded input is
// still its own real path (R1a), and the subject filter judges the path as
// recorded (R2).

const MERGE_HDR: &str =
    "#pragma once\n#ifndef SEEN\n#define SEEN\n#define V 1\n#else\n#undef V\n#define V 2\n#endif\n";
const MERGE_PRE: &str = "#include <a.h>\n#include <b.h>\n#if V == 2\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
/// The edit that compensates `a.h` and `b.h` becoming one file (the
/// second include then skipped by `#pragma once`: `V` stays 1).
const MERGE_POST: &str = "#include <a.h>\n#include <b.h>\n#if V == 1\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";

/// `ext/a.h` and `ext/b.h` are two files of the same bytes, both read.
fn merge_tree(name: &str) -> (Tree, Observer, Shard) {
    cfg_tree_with(name, MERGE_PRE, MERGE_POST, &["ext"], |t| {
        write(&t.base.join("ext/a.h"), MERGE_HDR);
        write(&t.base.join("ext/b.h"), MERGE_HDR);
    })
}

fn moved(p: &Path, real: &Path) -> String {
    format!(
        "pre-image: {} now leads to {}, not the file the map parse read (run `slip parse` again)",
        p.display(),
        real.display()
    )
}

// R1, the pair: nothing changed since the map phase; the edit is seen.
#[test]
fn o2_merged_inputs_pair() {
    let (tree, o, _) = merge_tree("o2-merge-pair");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// R1a: `ext/a.h` replaced by a symbolic link to the identical `ext/b.h`:
// same size and bytes, but its real path is now `ext/b.h`.
#[test]
fn o2_input_merged_by_symbolic_link() {
    let (tree, o, _) = merge_tree("o2-merge-symlink");
    let a = tree.base.join("ext/a.h");
    std::fs::remove_file(&a).unwrap();
    link("b.h", &a);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), moved(&a, &tree.base.join("ext/b.h")));
}

// R1b, the stated limit (spec/verify.md §5.1): `ext/a.h` replaced by a
// hard link of the identical `ext/b.h`. Its path, size and bytes are as
// recorded, and two inputs that are one file at the map phase are
// legitimate (deduplicated stores), so nothing recorded tells this from a
// good tree: the verdict is `verified` in a world the map phase never saw.
// Closing it needs the map shard to record which inputs were one file:
// WP-135. This test records today's answer; WP-135 turns it into a refusal.
#[test]
fn o2_limit_inputs_merged_by_hard_link_wp135() {
    let (tree, o, _) = merge_tree("o2-merge-hardlink");
    let a = tree.base.join("ext/a.h");
    std::fs::remove_file(&a).unwrap();
    std::fs::hard_link(tree.base.join("ext/b.h"), &a).unwrap();
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Verified, "{}", reason(&v));
}

// R2: the directory of an unread input (named by an `-I` word) replaced by
// a link to the subject root: G5 refuses the post parse's route.
#[test]
fn o2_unread_input_directory_linked_to_the_subject() {
    let (tree, o, _) = dropped_include("o2-unread-link-subject");
    let ext = tree.base.join("ext");
    std::fs::rename(&ext, tree.base.join("ext-old")).unwrap();
    link(tree.subject.to_str().unwrap(), &ext);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
}

// R2b: as R2, the link to a directory under the subject root.
#[test]
fn o2_unread_input_directory_linked_under_the_subject() {
    let (tree, o, _) = dropped_include("o2-unread-link-subject-dir");
    std::fs::create_dir_all(tree.subject.join("inc")).unwrap();
    std::fs::create_dir_all(tree.post.join("inc")).unwrap();
    let ext = tree.base.join("ext");
    std::fs::rename(&ext, tree.base.join("ext-old")).unwrap();
    link(tree.subject.join("inc").to_str().unwrap(), &ext);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
}

// R2's control: the directory removed: `is gone`.
#[test]
fn o2_unread_input_directory_removed() {
    let (tree, o, _) = dropped_include("o2-unread-dir-removed");
    std::fs::rename(tree.base.join("ext"), tree.base.join("ext-old")).unwrap();
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), left(&tree.base.join("ext/old.h"), "is gone"));
}

/// `a.cpp` reads `../ext/old.h` by name (no command word); the post
/// `a.cpp` no longer does.
const RELATIVE_PRE: &str = "#include \"../ext/old.h\"\nint f() { return 1; }\n";

// R2c: the directory replaced by a link to a directory under the subject
// root: `is gone` (and the pair, and the same with the link removed).
#[test]
fn o2_unread_relative_input_directory_linked_under_the_subject() {
    let (tree, o, _) =
        cfg_tree_with("o2-relative-link-subject-dir", RELATIVE_PRE, OLD_POST, &[], |t| {
            write(&t.base.join("ext/old.h"), "// v1\n");
            std::fs::create_dir_all(t.subject.join("inc")).unwrap();
        });
    std::fs::create_dir_all(tree.post.join("inc")).unwrap();
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Verified, "pair: {}", reason(&v));
    let ext = tree.base.join("ext");
    let gone = left(&ext.join("old.h"), "is gone");
    std::fs::rename(&ext, tree.base.join("ext-old")).unwrap();
    link(tree.subject.join("inc").to_str().unwrap(), &ext);
    let v = tree.verify(&o);
    assert_eq!((kind(&v), reason(&v)), (VerdictKind::Unverifiable, gone.as_str()));
    std::fs::remove_file(&ext).unwrap();
    let v = tree.verify(&o);
    assert_eq!((kind(&v), reason(&v)), (VerdictKind::Unverifiable, gone.as_str()));
}

// R2d: as R2c, the link to the subject root itself. Judged through the
// link, `ext/old.h` would be under the subject and skipped; it is judged
// as recorded, under no root, and it is gone.
#[test]
fn o2_unread_relative_input_directory_linked_to_the_subject() {
    let (tree, o, _) =
        cfg_tree_with("o2-relative-link-subject", RELATIVE_PRE, OLD_POST, &[], |t| {
            write(&t.base.join("ext/old.h"), "// v1\n");
        });
    let ext = tree.base.join("ext");
    std::fs::rename(&ext, tree.base.join("ext-old")).unwrap();
    link(tree.subject.to_str().unwrap(), &ext);
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), left(&ext.join("old.h"), "is gone"));
}

// ----- N1, re-check round of WP-117 (R3): a search directory Kiln dropped
// as a duplicate of a listed one is a place as an absent one is.

/// `-Ia -Ilnk -Iext`, `lnk -> a` at the map phase (Kiln drops `lnk`);
/// `b/cfg.h` (`USE_NEW 1`) is there for a retarget.
fn duplicate_tree(name: &str) -> (Tree, Observer, Shard) {
    cfg_tree_with(name, CFG_PRE, CFG_POST, &["a", "lnk", "ext"], |t| {
        std::fs::create_dir_all(t.base.join("a")).unwrap();
        write(&t.base.join("b/cfg.h"), "#define USE_NEW 1\n");
        link("a", &t.base.join("lnk"));
    })
}

#[test]
fn n1_duplicate_directory_pair() {
    let (tree, o, m) = duplicate_tree("n1-duplicate-pair");
    let negatives = m.negatives.as_ref().unwrap();
    assert!(negatives.contains(&bytes(&tree.base.join("lnk/cfg.h"))), "{negatives:?}");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Mismatch, "{}", reason(&v));
}

// R3: `lnk` retargeted to `b` after the map phase: `lnk/cfg.h` appeared.
#[test]
fn n1_duplicate_directory_retargeted() {
    let (tree, o, _) = duplicate_tree("n1-duplicate-retargeted");
    std::fs::remove_file(tree.base.join("lnk")).unwrap();
    link("b", &tree.base.join("lnk"));
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
    assert_eq!(reason(&v), appeared(&tree.base.join("lnk/cfg.h")));
}
