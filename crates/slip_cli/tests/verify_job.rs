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

// §5.3 G1: through an absolute `-I` into the subject that does not spell
// the subject root, so the post command keeps it.
#[test]
fn g1_absolute_include_directory() {
    let tree = Tree::new("g1absi", &[("a.cpp", "#include \"h.h\"\n"), ("inc/h.h", "int a = 1;\n")]);
    let dir = format!("-I{}/./s/inc", tree.base.to_str().unwrap());
    let o = tree.observer_of("a.cpp", &[&dir]);
    tree.map(&o);
    let v = tree.verify(&o);
    let h = tree.path("inc/h.h");
    assert_eq!(reason(&v), format!("post-image: read {h}, a file of the subject"));
}

// §5.3 G1: through an include directory relative to the post root,
// which the post command does not rewrite.
#[test]
fn g1_relative_include_directory() {
    let tree = Tree::new("g1rel", &[("a.cpp", "#include \"h.h\"\n"), ("inc/h.h", "int a = 1;\n")]);
    let o = tree.observer_of("a.cpp", &["-I../s/inc"]);
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
    for (case, one, two) in [
        ("tu", "int a = 1;\n", "int a = 2;\n"),
        ("inputs", "int a = 1;\n", "int a = 1; \n"),
    ] {
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
            std::fs::write(&tmp, if n % 2 == 0 { one } else { two }).unwrap();
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
const ADL: &str = "namespace a { struct X {}; int h(X); }\nint h(a::X, long);\nint h(long);\n\
int f() { a::X x; return h(x) + h(2); }\n";
const CONSTANTS: &str = "enum E { e0, e1 };\nconstexpr int k = 3;\nunsigned char c = 200;\n\
int f() { return k + e1; }\n";
const MACROS: &str = "#define V 1\n#define W(x) x\nint f() { return W(V); }\n";

/// The edits of the sweep that verify, each argued from the standard.
///
/// - `use(S s)` to `use(::S s)`: `::S` is looked up in the global
///   namespace ([basic.lookup.qual.general]) and names the class `S` that
///   `S` names; the parameter type, and so the function's type
///   ([dcl.fct]), and its body are the same. Nothing that resolves
///   through the declaration changes.
///
/// Edits that keep the meaning and are a mismatch (conservative, not a
/// wrong `verified`): `h(x)` to `a::h(x)` (the same function, but a
/// qualified name suppresses argument-dependent lookup,
/// [basic.lookup.argdep] p1, so the call is not the same expression). The
/// `templates` tree is a mismatch for every edit, and is with no edit at all:
/// its units are indeterminate until WP-045.
const SWEEP_VERIFIES: &[(&str, &str)] = &[("struct", "use(S s)")];

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
        if got.as_ref().is_some_and(|(k, _)| *k == VerdictKind::Verified) {
            verified.push((name, from));
        }
    }
    assert_eq!(verified, SWEEP_VERIFIES, "edits that verify");
}
