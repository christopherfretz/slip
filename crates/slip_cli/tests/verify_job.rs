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
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp052-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let subject = base.join("s");
        let post = base.join("post-tree");
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
        Params { pre: 0, declared, image: self.image(), limit: 64, locate: 16 }
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
    assert!(verdict.findings.iter().all(|f| f.location.is_none()));
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
        g3_case(case, one, two);
    }
}

fn g3_case(case: &str, one: &'static str, two: &'static str) {
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
    let mut seen = None;
    for attempt in 0..200 {
        let v = tree.verify(&o);
        assert_ne!(kind(&v), VerdictKind::Verified, "{case}");
        if kind(&v) == VerdictKind::Unverifiable {
            assert_eq!(reason(&v), "post-image: changed during the job", "{case}");
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

// F4b (re-check): a header added outside both roots after the map phase,
// in a directory searched before the one the map read it from, with an
// edit that compensates. O2 compares only files both parses read; the map
// parse's failed lookups are not kept ("negative inputs", WP-117): §5.1
// "What remains outside".
#[test]
#[ignore = "needs: WP-117 (negative inputs in the map shard)"]
fn f4b_outside_header_added_after_the_map() {
    let pre = "#include <cfg.h>\n#if USE_NEW\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
    let post = "#include <cfg.h>\n#if !USE_NEW\nint f() { return 2; }\n#else\nint f() { return 1; }\n#endif\n";
    let tree = Tree::new("f4b", &[("a.cpp", pre)]);
    write(&tree.post.join("a.cpp"), post);
    std::fs::create_dir_all(tree.base.join("ext2")).unwrap();
    write(&tree.base.join("ext/cfg.h"), "#define USE_NEW 0\n");
    let o = tree.observer_in(
        &tree.subject,
        &[arg("-I", &tree.base.join("ext2")), arg("-I", &tree.base.join("ext"))],
    );
    tree.map(&o);
    write(&tree.base.join("ext2/cfg.h"), "#define USE_NEW 1\n");
    let v = tree.verify(&o);
    assert_eq!(kind(&v), VerdictKind::Unverifiable, "{}", reason(&v));
}
