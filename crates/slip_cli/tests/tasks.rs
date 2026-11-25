//! `docs/wp/WP-074.md` "Tests": the worker's two tasks of a transform's
//! run, in process, with no coordinator. The map task with a transform
//! (`spec/edits.md` §8.1), reduce (`slip_cli::transform::reduce`), the
//! overlay built by hand with `slip_apply::build` (`spec/edits.md` §7.1),
//! and the verify task through it (`spec/verify.md` §5.1 "The overlay
//! form", §5.3 O1 and O2, §9.1, §10.4), over a small tree and a store in a
//! directory of their own under the work root (`common::work_root()`).
//!
//! A parse changes the process's current directory: every test that
//! parses holds [`LOCK`], and every path is absolute.

mod common;

use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_cli::image::Image;
use slip_cli::transform::reduce::reduce_round;
use slip_cli::verify_job::{Params, verify};
use slip_cli::worker::post::Loaded;
use slip_cli::worker::{MapTask, VerifyTask};
use slip_exec::store::{Access, Object, Store};
use slip_exec::worker::{Job, Post, PostImage, Task};
use slip_model::declared::Declared;
use slip_model::edits::Piece;
use slip_model::editset::{EditSet, SetState};
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Overlay, OverlayFile};
use slip_model::transform::{Ownership, Rename, Script, TargetKind, Transform, TransformKind};
use slip_model::verdict::VerdictKind;
use slip_model::{
    Command, Configuration, Digest, FileRef, Language, Observer, Origin, Phase, Plan, Shard, Status,
};

static LOCK: Mutex<()> = Mutex::new(());

/// The round of the transform's run: it maps and verifies in one round
/// (`spec/edits.md` §8), so the verify job's `pre` is this round too.
const ROUND: u64 = 1;

/// The subject. `f` is declared in `h.h` and again in `d.h`; `a.cpp`
/// reads both and calls `f`; `b.cpp` reads `d.h` only and names nothing;
/// `e.cpp` reads no file the rename edits. `h` is another function.
/// `c.cpp`, `w.h` and `v.h` are for the overlay made by hand (an edit that
/// adds an `#include`).
const FILES: &[(&str, &str)] = &[
    ("h.h", "int f();\nint h();\n"),
    ("d.h", "int f();\n"),
    ("a.cpp", "#include \"h.h\"\n#include \"d.h\"\nint x = f();\n"),
    ("b.cpp", "#include \"d.h\"\nint y = 0;\n"),
    ("e.cpp", "int z = 3;\n"),
    ("c.cpp", "#include \"w.h\"\nint c = W;\n"),
    ("w.h", "#define W 1\n"),
    ("v.h", "#define V 2\n"),
];

/// `c.cpp` after the hand-made edit: it includes `v.h`, whose overlaid
/// content defines `V` as `W` was; with `v.h` read from disk instead, `c`
/// would be 2.
const C_POST: &str = "#include \"w.h\"\n#include \"v.h\"\nint c = V;\n";
const V_POST: &str = "#define V 1\n";

fn bytes(p: &Path) -> Vec<u8> {
    p.as_os_str().as_bytes().to_vec()
}

fn content(b: &[u8]) -> Content {
    Content { size: b.len() as u64, content: content_digest(b) }
}

/// The pre-image on disk, for `slip_apply::build`.
struct Disk;

impl slip_apply::Files for Disk {
    fn read(&self, path: &[u8]) -> std::io::Result<Vec<u8>> {
        std::fs::read(std::ffi::OsStr::from_bytes(path))
    }

    fn identity(&self, path: &[u8]) -> std::io::Result<Option<(u64, u64)>> {
        slip_apply::Files::identity(&slip_cli::transform::store::DiskFiles, path)
    }
}

/// A tree `s/` and a store in a directory of their own, removed on drop;
/// the lock is held while it lives. The plan has one observer per main
/// file.
struct Tree {
    base: PathBuf,
    subject: PathBuf,
    store: Store,
    plan: Plan,
    plan_id: Digest,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for Tree {
    fn drop(&mut self) {
        // The bind mount sometimes answers ENOTEMPTY for a directory just
        // emptied: try again, as the other tests do.
        for _ in 0..3 {
            if std::fs::remove_dir_all(&self.base).is_ok() || !self.base.exists() {
                return;
            }
        }
    }
}

const MAINS: [&str; 4] = ["a.cpp", "b.cpp", "e.cpp", "c.cpp"];

impl Tree {
    fn new(name: &str) -> Tree {
        Tree::new_in(&common::work_root(), name)
    }

    /// A tree under `root`: the work root, or the folding root for a test
    /// about spelling on a volume that folds (`common::folding_root`).
    fn new_in(root: &Path, name: &str) -> Tree {
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = root.join(format!("wp074-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let subject = base.join("s");
        std::fs::create_dir_all(&subject).unwrap();
        for (rel, text) in FILES {
            std::fs::write(subject.join(rel), text).unwrap();
        }
        let store = Store::open(&base.join("store"), Access::Shared).unwrap();
        let mut observers: Vec<Observer> =
            MAINS.iter().enumerate().map(|(i, m)| observer_of(&subject, m, i as u64)).collect();
        observers.sort_by_key(|o| o.id);
        let plan = Plan {
            tool_slip: "s".into(),
            tool_kiln: "k".into(),
            configurations: vec![Configuration {
                name: "default".into(),
                database: FileRef {
                    path: b"/cdb.json".to_vec(),
                    size: 1,
                    content: Digest([1; 32]),
                },
            }],
            observers,
            excluded: vec![],
        };
        let plan_id = store.publish_plan(&plan.to_bytes()).unwrap();
        Tree { base, subject, store, plan, plan_id, _lock: lock }
    }

    fn path(&self, rel: &str) -> Vec<u8> {
        bytes(&self.subject.join(rel))
    }

    fn observer(&self, main: &str) -> Observer {
        let main = self.path(main);
        self.plan.observers.iter().find(|o| o.main.path == main).unwrap().clone()
    }

    fn ownership(&self) -> Ownership {
        Ownership { owned: vec![bytes(&self.subject)], readonly: vec![] }
    }

    /// The rename of `f` to `g` over the subject, published.
    fn rename(&self) -> (Transform, Digest) {
        let t = Transform {
            kind: TransformKind::Rename,
            ownership: self.ownership(),
            indivisible: true,
            rename: Some(Rename {
                target: vec![b"f".to_vec()],
                kind: TargetKind::Any,
                new_name: b"g".to_vec(),
                dependent: slip_model::transform::Dependent::default(),
            }),
            script: None,
        };
        let id = self.store.publish_object(Object::Transform, &t.to_bytes()).unwrap();
        (t, id)
    }

    fn job(
        &self,
        o: &Observer,
        phase: Phase,
        transform: Option<Digest>,
        post: Option<Post>,
    ) -> Job {
        Job {
            store: bytes(self.store.root()),
            plan: self.plan_id,
            phase,
            round: ROUND,
            observer: o.clone(),
            transform,
            post,
        }
    }

    /// The map job of `o` with the request `id`, its shard validated.
    fn map(&self, o: &Observer, id: Option<Digest>) -> Result<Shard, String> {
        let shard = MapTask::new().run(&self.job(o, Phase::Map, id, None))?;
        shard.validate().unwrap();
        Ok(shard)
    }

    /// Every observer mapped with the rename and published; reduce over
    /// them; the edit set and its declared changes.
    fn map_and_reduce(&self) -> Reduced {
        let (t, id) = self.rename();
        for o in &self.plan.observers {
            let shard = self.map(o, Some(id)).unwrap();
            assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
            self.store
                .publish_shard(&self.plan_id, Phase::Map, ROUND, &o.id, &shard.to_bytes())
                .unwrap();
        }
        let selected: Vec<usize> = (0..self.plan.observers.len()).collect();
        let (set_id, set, declared) =
            reduce_round(&self.store, self.plan_id, &self.plan, ROUND, &selected, (id, &t), 1)
                .unwrap();
        assert_eq!(set.state, SetState::Accepted, "{set:?}");
        let (declared_id, declared) = declared.expect("an accepted set has declared changes");
        Reduced { set_id, set, declared_id, declared }
    }

    /// The overlay `slip_apply::build` makes of `set`, with its blobs,
    /// published.
    fn publish_overlay(&self, set: &EditSet, set_id: Digest) -> (Digest, Overlay) {
        let built = slip_apply::build(set, set_id, &Disk).unwrap();
        for (_, b) in &built.blobs {
            self.store.publish_blob(b).unwrap();
        }
        let id = self.store.publish_object(Object::Overlay, &built.overlay.to_bytes()).unwrap();
        (id, built.overlay)
    }

    /// `overlay` published as it is, with the blobs of `blobs`.
    fn put_overlay(&self, overlay: &Overlay, blobs: &[&[u8]]) -> Digest {
        overlay.validate().unwrap();
        for b in blobs {
            self.store.publish_blob(b).unwrap();
        }
        self.store.publish_object(Object::Overlay, &overlay.to_bytes()).unwrap()
    }

    /// The verify job of `o` through the overlay `overlay` with the
    /// declared changes `declared`, by the verify task (the worker's own
    /// path from `job.post`).
    fn verify_task(
        &self,
        o: &Observer,
        overlay: Digest,
        declared: Digest,
    ) -> Result<Shard, String> {
        let post = Post { pre: ROUND, declared, image: PostImage::Overlay(overlay) };
        let shard = VerifyTask::new(64, 16).run(&self.job(o, Phase::Verify, None, Some(post)))?;
        shard.validate().unwrap();
        Ok(shard)
    }

    /// The overlay made by hand: `c.cpp` edited to include `v.h`, `v.h`
    /// overlaid; with `extra`, more files the observer never reads
    /// (`(path, pre, post)`, sorted in).
    fn include_overlay(&self, extra: &[OverlayFile]) -> Overlay {
        let file = |rel: &str, post: &str| {
            let pre = std::fs::read(self.subject.join(rel)).unwrap();
            OverlayFile { path: self.path(rel), pre: content(&pre), post: content(post.as_bytes()) }
        };
        let mut files = vec![file("c.cpp", C_POST), file("v.h", V_POST)];
        files.extend_from_slice(extra);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Overlay { edits: Digest([0x0e; 32]), files, spellings: Vec::new() }
    }

    /// `c.cpp` mapped (no transform: the hand-made overlay has no request)
    /// and published; the declared changes of no rule, published.
    fn map_c(&self) -> (Observer, Shard, Digest) {
        let o = self.observer("c.cpp");
        let shard = self.map(&o, None).unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
        self.store
            .publish_shard(&self.plan_id, Phase::Map, ROUND, &o.id, &shard.to_bytes())
            .unwrap();
        let declared = self.store.publish_declared(&Declared::none().to_bytes()).unwrap();
        (o, shard, declared)
    }

    /// The observer of `main` (a file of the subject made by the test, not
    /// in the plan) mapped without a transform and published; the
    /// declared changes of no rule, published.
    fn map_publish(&self, main: &str, entry: u64) -> (Observer, Digest) {
        let o = observer_of(&self.subject, main, entry);
        let shard = self.map(&o, None).unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{main}: {:?}", shard.outcome);
        self.store
            .publish_shard(&self.plan_id, Phase::Map, ROUND, &o.id, &shard.to_bytes())
            .unwrap();
        let declared = self.store.publish_declared(&Declared::none().to_bytes()).unwrap();
        (o, declared)
    }

    /// The verify job of `o` through the overlay `ovly` by `verify` itself,
    /// with the test hook `with_loading_limit(limit)` when given.
    fn verify_limit(
        &self,
        o: &Observer,
        ovly: Digest,
        declared: Digest,
        limit: Option<usize>,
    ) -> Result<Shard, String> {
        let decl =
            Declared::from_bytes(&self.store.read_declared(&declared).unwrap().unwrap()).unwrap();
        let mut image = Image::from_post(&self.store, &PostImage::Overlay(ovly)).unwrap();
        if let Some(n) = limit {
            image = image.with_loading_limit(n);
        }
        let params = Params {
            pre: ROUND,
            declared: decl,
            declared_id: declared,
            image,
            limit: 64,
            locate: 16,
        };
        verify(&self.store, &self.job(o, Phase::Verify, None, None), &params)
    }

    /// An overlay entry for `rel`: `pre` and `post` bytes.
    fn entry(&self, rel: &str, pre: &[u8], post: &[u8]) -> OverlayFile {
        OverlayFile { path: self.path(rel), pre: content(pre), post: content(post) }
    }

    /// The subject's files as they are now.
    fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
        FILES
            .iter()
            .map(|(rel, _)| ((*rel).to_owned(), std::fs::read(self.subject.join(rel)).unwrap()))
            .collect()
    }
}

struct Reduced {
    set_id: Digest,
    set: EditSet,
    declared_id: Digest,
    declared: Declared,
}

fn observer_of(subject: &Path, main: &str, entry: u64) -> Observer {
    let path = bytes(&subject.join(main));
    let command = Command {
        directory: bytes(subject),
        argv: vec![b"c++".to_vec(), b"-std=c++20".to_vec(), path.clone()],
        env: Vec::new(),
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::Cxx,
        main: FileRef { path, size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry }],
    }
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

/// The overlay without the file at `path`: its obligations dropped, those
/// of later files renumbered (a structural edit of the record, as the
/// package's "one declaration left out of the overlay" asks).
fn without_file(overlay: &Overlay, path: &[u8]) -> Overlay {
    let at = overlay.find(path).expect("the overlay names the file") as u64;
    let mut out = overlay.clone();
    out.files.remove(at as usize);
    out.spellings = overlay
        .spellings
        .iter()
        .filter(|o| o.file != at)
        .map(|o| {
            let mut o = *o;
            if o.file > at {
                o.file -= 1;
            }
            o
        })
        .collect();
    out
}

// spec/edits.md §8.1, §6.3: a map job with a rename request. The shard has
// `inputs`, `digests` and `edits` with the `edits.*` counters; its
// `digests` equal those of the job without a transform, and without the
// `edits` section and counters it is that shard, byte for byte; the edits
// name the request.
#[test]
fn s8_1_map_with_a_rename() {
    let tree = Tree::new("map");
    let (_, id) = tree.rename();
    let a = tree.observer("a.cpp");
    let with = tree.map(&a, Some(id)).unwrap();
    let without = tree.map(&a, None).unwrap();
    assert_eq!(with.outcome.status, Status::Ok, "{:?}", with.outcome);
    assert!(with.inputs.is_some());
    let edits = with.edits.as_ref().expect("an ok shard of a transform has edits");
    assert_eq!(edits.transform, id);
    assert!(!edits.claims.is_empty(), "the rename claims f's names");
    for n in ["edits.claims", "edits.changed", "edits.files", "edits.refs", "edits.subjects"] {
        assert!(counter(&with, n).is_some(), "{n}");
    }
    let names: Vec<&str> = with.counters.iter().map(|c| c.name.as_str()).collect();
    assert!(names.windows(2).all(|w| w[0] < w[1]), "{names:?}");
    assert_eq!(with.digests, without.digests);
    assert!(without.edits.is_none());
    let mut stripped = with.clone();
    stripped.edits = None;
    stripped.counters.retain(|c| !c.name.starts_with("edits."));
    assert_eq!(stripped.to_bytes(), without.to_bytes());
}

// docs/wp/WP-074.md "Done when": a map job without a transform is M1's:
// its shard is the map task's of before, byte for byte (the same as
// `map_observer`, M1's entry point, which WP-050's tests pin), and two runs
// of it are equal.
#[test]
fn s8_5_map_without_a_transform_is_m1s() {
    let tree = Tree::new("m1");
    let a = tree.observer("a.cpp");
    let task = tree.map(&a, None).unwrap();
    let m1 = slip_cli::worker::map_observer(&a, ROUND, true).unwrap();
    assert_eq!(task.to_bytes(), m1.to_bytes());
    assert!(task.edits.is_none());
    assert!(task.counters.iter().all(|c| !c.name.starts_with("edits.")));
}

// spec/edits.md §8.1: a unit with an error has no `edits`; a script
// request fails the job with the message of WP-074 (WP-092 not landed);
// a request whose file is not its content id, or that is not in the
// store, fails the job.
#[test]
fn s8_1_map_refusals() {
    let tree = Tree::new("map-refuse");
    let (t, id) = tree.rename();
    std::fs::write(tree.subject.join("bad.cpp"), "int f();\nint x = nope;\n").unwrap();
    let bad = observer_of(&tree.subject, "bad.cpp", 9);
    let shard = tree.map(&bad, Some(id)).unwrap();
    assert_eq!(shard.outcome.status, Status::Diagnosed, "{:?}", shard.outcome);
    assert!(shard.edits.is_none());
    assert!(shard.counters.iter().all(|c| !c.name.starts_with("edits.")));

    let script = Transform {
        kind: TransformKind::Script,
        rename: None,
        script: Some(Script {
            id: Digest([0x5c; 32]),
            file: tree.path("t.cc"),
            options: Vec::new(),
            steps: 1 << 30,
            bytes: 1 << 30,
            depth: 512,
        }),
        indivisible: false,
        ..t.clone()
    };
    let script_id = tree.store.publish_object(Object::Transform, &script.to_bytes()).unwrap();
    let a = tree.observer("a.cpp");
    assert_eq!(
        tree.map(&a, Some(script_id)).unwrap_err(),
        "a script transform needs a worker built with slip_script"
    );
    // Refused before anything is parsed (review R11): an observer whose
    // main file is not there gets the refusal, not the parse's
    // `diagnosed` shard, which a refusal after the parse would leave.
    let nowhere = observer_of(&tree.subject, "nowhere.cpp", 10);
    assert_eq!(
        tree.map(&nowhere, Some(script_id)).unwrap_err(),
        "a script transform needs a worker built with slip_script"
    );
    assert_ne!(tree.map(&nowhere, None).unwrap().outcome.status, Status::Ok);

    // Another request's bytes under this id.
    let other = Digest([0x77; 32]);
    let path = tree.store.object_path(Object::Transform, &other);
    std::fs::write(&path, t.to_bytes()).unwrap();
    let e = tree.map(&a, Some(other)).unwrap_err();
    assert!(e.ends_with(&format!("has the content id {id}, not {other}")), "{e}");
    let missing = Digest([0x78; 32]);
    let e = tree.map(&a, Some(missing)).unwrap_err();
    assert!(e.starts_with(&format!("the request {missing} is not in the store")), "{e}");
}

// spec/verify.md §5.1 "The overlay form", §9.1, §10.4: the overlay a
// correct rename makes. Every observer is `verified`; `binding` names the
// overlay, the declared changes and the pre round; `rule_use` has `via`
// for the rule where the observer sees the renamed function; `spellings`
// has every obligation of the files read as held; the tree on disk is
// untouched.
#[test]
fn s5_1_verify_a_correct_rename() {
    let tree = Tree::new("correct");
    let r = tree.map_and_reduce();
    let (ovly, overlay) = tree.publish_overlay(&r.set, r.set_id);
    assert!(!overlay.spellings.is_empty(), "the rename's names are obligations");
    let before = tree.snapshot();
    for main in ["a.cpp", "b.cpp", "e.cpp", "c.cpp"] {
        let o = tree.observer(main);
        let shard = tree.verify_task(&o, ovly, r.declared_id).unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{main}: {:?}", shard.outcome);
        assert_eq!(kind(&shard), VerdictKind::Verified, "{main}: {}", reason(&shard));
        let binding = shard.binding.expect("binding");
        assert_eq!(binding.overlay, Some(ovly));
        assert_eq!(binding.declared, r.declared_id);
        assert_eq!(binding.pre, ROUND);
        let rule_use = shard.rule_use.as_ref().expect("rule_use under rules");
        let spellings = shard.spellings.as_ref().expect("spellings: the overlay has obligations");
        let inputs = shard.inputs.as_ref().unwrap();
        let expected: u64 = inputs
            .iter()
            .filter_map(|f| overlay.find(&f.path))
            .map(|i| overlay.obligations_of(i).len() as u64)
            .sum();
        assert_eq!(spellings.seen, expected, "{main}");
        let held: u64 = spellings.held.iter().map(|s| s.count).sum();
        assert_eq!(held, expected, "{main}: every obligation seen holds");
        assert!(spellings.failed.is_empty(), "{main}: {:?}", spellings.failed);
        assert_eq!(counter(&shard, "verify.spelling.obligations"), Some(expected));
        assert_eq!(counter(&shard, "verify.spelling.holds"), Some(expected));
        assert_eq!(counter(&shard, "verify.spelling.fails"), Some(0));
        assert_eq!(counter(&shard, "verify.spelling.not_seen"), Some(0));
        let via: u64 = rule_use.uses.iter().map(|u| u.via).sum();
        match main {
            "a.cpp" | "b.cpp" => assert!(via > 0, "{main}: {rule_use:?}"),
            _ => assert!(rule_use.uses.is_empty(), "{main}: {rule_use:?}"),
        }
        // `a.cpp`'s call of `f` is a reference: an obligation.
        if main == "a.cpp" {
            assert!(expected > 0, "a.cpp reads a reference the rename spells");
        }
    }
    assert_eq!(tree.snapshot(), before, "the verify job writes no file of the subject");
}

// spec/verify.md §9.3, §10.4: the verify job's shard is the report's when
// the run's overlay is the one it was bound to, and `failed` with its
// reason when the run's overlay is another (the same records under another
// id, so that the binding is the only difference).
#[test]
fn s9_3_a_shard_bound_to_another_overlay() {
    use slip_verify::run::{Header, MapShard, Report, State};
    let tree = Tree::new("other-overlay");
    let r = tree.map_and_reduce();
    let (ovly, overlay) = tree.publish_overlay(&r.set, r.set_id);
    let o = tree.observer("a.cpp");
    let shard = tree.verify_task(&o, ovly, r.declared_id).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    // The map shard the verdict was made against (WP-114: `Report` reads
    // its status and content id).
    let bytes = tree.store.read_shard(&tree.plan_id, Phase::Map, ROUND, &o.id).unwrap().unwrap();
    let (map, map_id) = Shard::from_bytes_with_id(&bytes).unwrap();
    let map = MapShard::Read { status: map.outcome.status, id: map_id };
    let report = |run_overlay: Digest| {
        let header = Header {
            plan: tree.plan_id,
            round: ROUND,
            declared: r.declared_id,
            rules: r.declared.rules_id(),
            image: Vec::new(),
            limit: 64,
            locate: 16,
            plan_observers: 1,
            run: b"/run".to_vec(),
        };
        let mut report = Report::new(header, &r.declared);
        report.pre_round(ROUND);
        report.overlay(run_overlay, &overlay);
        report.add(&o.id, &o.main.path, Some(&shard), Some(map));
        report
    };
    let ours = report(ovly);
    assert_eq!(ours.states()[State::Verified as usize], 1, "{}", ours.error_lines());
    let other = Digest([0xee; 32]);
    assert_ne!(other, ovly);
    let theirs = report(other);
    assert_eq!(theirs.states()[State::Failed as usize], 1);
    let main = String::from_utf8(o.main.path.clone()).unwrap();
    assert_eq!(
        theirs.error_lines(),
        format!("slip: failed: {main}: the shard's binding is not this run's: its overlay\n")
    );
}

// spec/verify.md §9.2, §5.2 step 5: the same with `d.h`, one declaration of
// `f`, left out of the overlay. `b.cpp` sees only that stale declaration:
// `verified`, with `direct` for the rule; `a.cpp` sees it and `h.h`'s
// renamed one: a mismatch (the collision).
#[test]
fn s9_2_a_declaration_left_out() {
    let tree = Tree::new("left-out");
    let r = tree.map_and_reduce();
    let (_, full) = tree.publish_overlay(&r.set, r.set_id);
    let overlay = without_file(&full, &tree.path("d.h"));
    let blobs: Vec<Vec<u8>> = slip_apply::build(&r.set, r.set_id, &Disk)
        .unwrap()
        .blobs
        .into_iter()
        .map(|(_, b)| b)
        .collect();
    let refs: Vec<&[u8]> = blobs.iter().map(Vec::as_slice).collect();
    let ovly = tree.put_overlay(&overlay, &refs);
    let b = tree.verify_task(&tree.observer("b.cpp"), ovly, r.declared_id).unwrap();
    assert_eq!(kind(&b), VerdictKind::Verified, "{}", reason(&b));
    let uses = &b.rule_use.as_ref().unwrap().uses;
    assert!(uses.iter().any(|u| u.direct > 0), "{uses:?}");
    let a = tree.verify_task(&tree.observer("a.cpp"), ovly, r.declared_id).unwrap();
    assert_eq!(kind(&a), VerdictKind::Mismatch, "{}", reason(&a));
    assert!(!r.declared.rules().is_empty());
}

// spec/verify.md §10.2, §10.4: one reference's replacement spelling a name
// that resolves to another function (`h`): `spellings.failed` has its
// obligation, with `h`'s key and name.
#[test]
fn s10_4_a_spelling_that_denotes_another() {
    let tree = Tree::new("spelling");
    let r = tree.map_and_reduce();
    let mut set = r.set.clone();
    let a_file = set.files.iter().position(|f| f.path == tree.path("a.cpp")).unwrap() as u64;
    let mut changed = 0;
    for edit in set.edits.iter_mut().filter(|e| e.file == a_file) {
        for piece in &mut edit.pieces {
            match piece {
                Piece::Identifier { text, .. } | Piece::Spelled { text, .. } if text == b"g" => {
                    *text = b"h".to_vec();
                    changed += 1;
                }
                _ => {}
            }
        }
    }
    assert_eq!(changed, 1, "a.cpp has one renamed reference: {:?}", set.edits);
    let set_id = tree.store.publish_object(Object::EditSet, &set.to_bytes()).unwrap();
    let (ovly, overlay) = tree.publish_overlay(&set, set_id);
    let a = tree.verify_task(&tree.observer("a.cpp"), ovly, r.declared_id).unwrap();
    assert_eq!(a.outcome.status, Status::Ok, "{:?}", a.outcome);
    let spellings = a.spellings.as_ref().expect("spellings");
    assert_eq!(spellings.failed.len(), 1, "{spellings:?}");
    let failed = &spellings.failed[0];
    let o = overlay.spellings[failed.index as usize];
    assert_eq!(overlay.files[o.file as usize].path, tree.path("a.cpp"));
    assert!(failed.found.is_some(), "h has a key: {failed:?}");
    assert!(failed.name.contains('h'), "{failed:?}");
    assert_eq!(counter(&a, "verify.spelling.fails"), Some(1));
    // Every obligation seen holds or fails (review R9: `not_seen` is not
    // `seen - holds`).
    let held: u64 = spellings.held.iter().map(|s| s.count).sum();
    assert_eq!(spellings.seen, held + 1, "{spellings:?}");
    assert_eq!(counter(&a, "verify.spelling.not_seen"), Some(0));
}

// spec/verify.md §5.1 "Loading": an overlay whose edit adds an `#include`
// of another overlaid file. The first parse is given `c.cpp` (the pre
// shard's inputs) and reads `v.h` from disk; the second is given both. The
// post-image is the right one (`V` is the overlay's 1, so the sums are the
// pre-image's), and the shard says nothing of the number of parses.
#[test]
fn s5_1_loading_an_included_overlay_file() {
    let tree = Tree::new("include");
    let (o, pre, declared) = tree.map_c();
    assert!(
        pre.inputs.as_ref().unwrap().iter().all(|f| f.path != tree.path("v.h")),
        "the map parse does not read v.h"
    );
    let overlay = tree.include_overlay(&[]);
    let ovly = tree.put_overlay(&overlay, &[C_POST.as_bytes(), V_POST.as_bytes()]);
    let shard = tree.verify_task(&o, ovly, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    let inputs = shard.inputs.as_ref().unwrap();
    let v = inputs.iter().find(|f| f.path == tree.path("v.h")).expect("v.h read");
    assert_eq!((v.size, v.content), (V_POST.len() as u64, content_digest(V_POST.as_bytes())));
    assert_eq!(counter(&shard, "verify.reparsed"), Some(0));
    assert!(shard.spellings.is_none(), "no obligation: no section 26");
    assert_eq!(shard.binding.unwrap().overlay, Some(ovly));
    // The same job again (the task keeps the overlay): the same bytes.
    let again = tree.verify_task(&o, ovly, declared).unwrap();
    assert_eq!(again.to_bytes(), shard.to_bytes());
}

// spec/verify.md §5.3 O1 and O2, §5.1 "Loading": a subject file changed on
// disk after the map job (O2); the loading loop stopped by the test hook
// after its first parse, so that `v.h` was read from disk (O1).
#[test]
fn s5_3_o1_and_o2() {
    let tree = Tree::new("o1o2");
    let (o, _, declared) = tree.map_c();
    let overlay = tree.include_overlay(&[]);
    let ovly = tree.put_overlay(&overlay, &[C_POST.as_bytes(), V_POST.as_bytes()]);

    let decl =
        Declared::from_bytes(&tree.store.read_declared(&declared).unwrap().unwrap()).unwrap();
    let image = Image::from_post(&tree.store, &PostImage::Overlay(ovly)).unwrap();
    let params = |image: Image| Params {
        pre: ROUND,
        declared: decl.clone(),
        declared_id: declared,
        image,
        limit: 64,
        locate: 16,
    };
    let job = tree.job(&o, Phase::Verify, None, None);
    let stopped = verify(&tree.store, &job, &params(image.clone().with_loading_limit(1))).unwrap();
    assert_eq!(kind(&stopped), VerdictKind::Unverifiable);
    let v = String::from_utf8(tree.path("v.h")).unwrap();
    assert_eq!(reason(&stopped), format!("post-image: {v} was not read through the overlay"));
    // Without the hook the same job verifies.
    let whole = verify(&tree.store, &job, &params(image.clone())).unwrap();
    assert_eq!(kind(&whole), VerdictKind::Verified, "{}", reason(&whole));

    // `w.h`, which the overlay does not name, changed since the map phase.
    std::fs::write(tree.subject.join("w.h"), "#define W 1 // changed\n").unwrap();
    let drift = verify(&tree.store, &job, &params(image)).unwrap();
    assert_eq!(kind(&drift), VerdictKind::Unverifiable);
    let w = String::from_utf8(tree.path("w.h")).unwrap();
    assert_eq!(reason(&drift), format!("pre-image: {w} changed since the map phase"));
}

// spec/verify.md §5.1 "Loading", spec/exec.md §5.4: an overlay blob
// replaced by other bytes, one missing from `blobs/`, and an overlay not
// in the store: each fails the job, never a shard.
#[test]
fn s5_1_blobs_that_fail_the_job() {
    let tree = Tree::new("blobs");
    let (o, _, declared) = tree.map_c();
    let overlay = tree.include_overlay(&[]);
    let ovly = tree.put_overlay(&overlay, &[C_POST.as_bytes(), V_POST.as_bytes()]);
    let v_blob = tree.store.blob_path(&content_digest(V_POST.as_bytes()));
    std::fs::write(&v_blob, "#define V 3\n").unwrap();
    let e = tree.verify_task(&o, ovly, declared).unwrap_err();
    assert!(e.contains("does not hash to its name"), "{e}");
    std::fs::remove_file(&v_blob).unwrap();
    let e = tree.verify_task(&o, ovly, declared).unwrap_err();
    assert!(e.contains(&format!("the blob {} of ", content_digest(V_POST.as_bytes()))), "{e}");
    assert!(e.contains("is not in the store"), "{e}");
    let c_blob = tree.store.blob_path(&content_digest(C_POST.as_bytes()));
    std::fs::remove_file(&c_blob).unwrap();
    let e = tree.verify_task(&o, ovly, declared).unwrap_err();
    assert!(e.contains("is not in the store"), "{e}");
    // An OVLY under another id.
    let other = Digest([0x99; 32]);
    std::fs::copy(
        tree.store.object_path(Object::Overlay, &ovly),
        tree.store.object_path(Object::Overlay, &other),
    )
    .unwrap();
    let e = tree.verify_task(&o, other, declared).unwrap_err();
    assert!(e.ends_with(&format!("has the content id {ovly}, not {other}")), "{e}");
}

// spec/verify.md §9.1, §10.4: an observer that reads no overlay file under a
// rule table that is not empty: `verified`, `rule_use` present and empty,
// `spellings` present with `seen` 0 (the overlay has obligations), no blob
// loaded (none is in the store); with an overlay that has no obligation,
// no section 26.
#[test]
fn s10_4_an_observer_that_reads_no_overlay_file() {
    let tree = Tree::new("none-read");
    let r = tree.map_and_reduce();
    let (ovly, overlay) = tree.publish_overlay(&r.set, r.set_id);
    for f in &overlay.files {
        std::fs::remove_file(tree.store.blob_path(&f.post.content)).unwrap();
    }
    let e = tree.observer("e.cpp");
    let shard = tree.verify_task(&e, ovly, r.declared_id).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    assert!(shard.rule_use.as_ref().unwrap().uses.is_empty());
    let spellings = shard.spellings.as_ref().expect("the overlay has obligations");
    assert_eq!((spellings.seen, spellings.held.len(), spellings.failed.len()), (0, 0, 0));
    assert_eq!(counter(&shard, "verify.spelling.obligations"), Some(0));

    let mut bare = overlay.clone();
    bare.spellings.clear();
    let bare_id = tree.put_overlay(&bare, &[]);
    let shard = tree.verify_task(&e, bare_id, r.declared_id).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    assert!(shard.spellings.is_none());
    assert!(counter(&shard, "verify.spelling.obligations").is_none());
    assert_eq!(shard.binding.unwrap().overlay, Some(bare_id));
}

// docs/wp/WP-074.md "Interface": the worker holds the bytes of the overlay
// files one observer reads, and no others. An overlay of ten thousand
// files of which `c.cpp`'s observer reads three: only those three blobs are
// in the store, and the job verifies; `Loaded` counts what it loaded.
#[test]
fn interface_loads_only_what_the_observer_reads() {
    let tree = Tree::new("ten-thousand");
    let (o, pre, declared) = tree.map_c();
    let w = std::fs::read(tree.subject.join("w.h")).unwrap();
    let mut extra: Vec<OverlayFile> = (0..9_997)
        .map(|i| OverlayFile {
            path: tree.path(&format!("gen/f{i:05}.h")),
            pre: content(b"x"),
            post: content(format!("int g{i};\n").as_bytes()),
        })
        .collect();
    // `w.h` overlaid with its own bytes: the third file read.
    extra.push(OverlayFile { path: tree.path("w.h"), pre: content(&w), post: content(&w) });
    let overlay = tree.include_overlay(&extra);
    assert_eq!(overlay.files.len(), 10_000);
    let ovly = tree.put_overlay(&overlay, &[C_POST.as_bytes(), V_POST.as_bytes(), &w]);
    let shard = tree.verify_task(&o, ovly, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));

    let mut loaded = Loaded::new(&overlay, ovly);
    let paths = pre.inputs.as_ref().unwrap();
    let n = loaded.load(&tree.store, &mut paths.iter().map(|f| f.path.as_slice())).unwrap();
    assert_eq!(n, 2, "c.cpp and w.h are the pre shard's");
    assert_eq!(loaded.bytes_loaded(), (C_POST.len() + w.len()) as u64);
    let v = tree.path("v.h");
    assert_eq!(loaded.load(&tree.store, &mut std::iter::once(v.as_slice())).unwrap(), 1);
    assert_eq!(loaded.load(&tree.store, &mut std::iter::once(v.as_slice())).unwrap(), 0);
    assert_eq!(loaded.files_loaded(), 3);
    assert_eq!(loaded.bytes_loaded(), (C_POST.len() + w.len() + V_POST.len()) as u64);
    assert!(loaded.load_all(&tree.store).unwrap_err().contains("is not in the store"));
}

// docs/wp/WP-114.md, WP-074 item 3: a verify job with a tree image never
// has `spellings`, with rules or without; it always has a `binding`, with
// no overlay (WP-114: every shard not `failed`).
#[test]
fn s10_4_a_tree_image_has_no_spellings() {
    let tree = Tree::new("tree");
    let r = tree.map_and_reduce();
    let none = tree.store.publish_declared(&Declared::none().to_bytes()).unwrap();
    for declared in [none, r.declared_id] {
        let post = Post { pre: ROUND, declared, image: Image::identity().to_post() };
        let o = tree.observer("a.cpp");
        let shard =
            VerifyTask::new(64, 16).run(&tree.job(&o, Phase::Verify, None, Some(post))).unwrap();
        shard.validate().unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
        assert!(shard.spellings.is_none());
        assert!(counter(&shard, "verify.spelling.obligations").is_none());
        let b = shard.binding.expect("WP-114: every shard not failed has a binding");
        assert_eq!(b.overlay, None);
        assert_eq!(b.declared, declared);
        assert_eq!(b.pre, ROUND);
    }
}

// spec/verify.md §5.1 "Loading", WP-066's F8: an overlay file reached by
// another path than the overlay's (a hard link: Kiln matches an entry by
// identity). `c.cpp`'s edit includes `v2.h`, a hard link of `v.h`, which
// the overlay names. The first parse reads `v2.h` from disk; it is the
// overlay file `v.h` by identity, so it is given and the observer parsed
// again; the second parse reads it through the overlay. Verified, with
// `v2.h`'s input the overlay's bytes.
#[test]
fn s5_1_loading_an_overlay_file_by_identity() {
    let tree = Tree::new("identity");
    // Two names of one file (identity); neither is replaced: the post-image
    // is the overlay's (spec/harness.md §11.4).
    std::fs::hard_link(tree.subject.join("v.h"), tree.subject.join("v2.h")).unwrap();
    let (o, _, declared) = tree.map_c();
    let c_post = "#include \"w.h\"\n#include \"v2.h\"\nint c = V;\n";
    let mut overlay = tree.include_overlay(&[]);
    let c = overlay.find(&tree.path("c.cpp")).unwrap();
    overlay.files[c].post = content(c_post.as_bytes());
    let ovly = tree.put_overlay(&overlay, &[c_post.as_bytes(), V_POST.as_bytes()]);
    let shard = tree.verify_task(&o, ovly, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    let inputs = shard.inputs.as_ref().unwrap();
    let v2 = inputs.iter().find(|f| f.path == tree.path("v2.h")).expect("v2.h read");
    assert_eq!(v2.content, content_digest(V_POST.as_bytes()));

    // The loop stopped after its first parse: `v2.h`, the overlay file
    // `v.h` by identity, was read from disk. O1's unit part names it; the
    // guard by path cannot (the overlay does not name `v2.h`, and the pre
    // shard does not list it), so without it the verdict would be a
    // comparison of the wrong post-image.
    let decl =
        Declared::from_bytes(&tree.store.read_declared(&declared).unwrap().unwrap()).unwrap();
    let image = Image::from_post(&tree.store, &PostImage::Overlay(ovly)).unwrap();
    let params = Params {
        pre: ROUND,
        declared: decl,
        declared_id: declared,
        image: image.with_loading_limit(1),
        limit: 64,
        locate: 16,
    };
    let stopped = verify(&tree.store, &tree.job(&o, Phase::Verify, None, None), &params).unwrap();
    assert_eq!(kind(&stopped), VerdictKind::Unverifiable);
    let v2 = String::from_utf8(tree.path("v2.h")).unwrap();
    assert_eq!(reason(&stopped), format!("post-image: {v2} was not read through the overlay"));
}

// WP-074's guards of the overlay form (spec/landed/exec/WP-074.md §5): an
// overlay made from another `c.cpp` than the map phase read (the subject's
// sums), and a header that appeared, after the map phase, where the map
// parse looked for one and found none (N1 over every negative input).
#[test]
fn s5_3_subject_sums_and_n1() {
    let tree = Tree::new("sums");
    let (o, _, declared) = tree.map_c();
    let c_cpp = String::from_utf8(tree.path("c.cpp")).unwrap();
    let c_pre = std::fs::read(tree.subject.join("c.cpp")).unwrap();
    // Another size; then the same size and other bytes (review R5: the
    // content is compared, not only the size).
    let mut same_size = c_pre.clone();
    same_size[c_pre.len() - 2] ^= 0x01;
    for pre in [b"int c = 1;\n".to_vec(), same_size] {
        let mut overlay = tree.include_overlay(&[]);
        let at = overlay.find(&tree.path("c.cpp")).unwrap();
        overlay.files[at].pre = content(&pre);
        let ovly = tree.put_overlay(&overlay, &[C_POST.as_bytes(), V_POST.as_bytes()]);
        let shard = tree.verify_task(&o, ovly, declared).unwrap();
        assert_eq!(kind(&shard), VerdictKind::Unverifiable);
        assert_eq!(
            reason(&shard),
            format!("pre-image: the overlay was made from another {c_cpp} than the map phase read")
        );
    }

    // `n.cpp` includes `<w.h>` searching `i/` before the subject: the map
    // parse looked for `i/w.h` and found none.
    let first = tree.base.join("i");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::write(tree.subject.join("n.cpp"), "#include <w.h>\nint n = W;\n").unwrap();
    let mut n_cpp = observer_of(&tree.subject, "n.cpp", 5);
    let flags = [format!("-I{}", first.display()), format!("-I{}", tree.subject.display())];
    n_cpp.command.argv.splice(2..2, flags.iter().map(|f| f.as_bytes().to_vec()));
    n_cpp.id = n_cpp.command.observer_id();
    let shard = tree.map(&n_cpp, None).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    tree.store
        .publish_shard(&tree.plan_id, Phase::Map, ROUND, &n_cpp.id, &shard.to_bytes())
        .unwrap();
    let good = tree.put_overlay(&tree.include_overlay(&[]), &[]);
    let shard = tree.verify_task(&n_cpp, good, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    std::fs::write(first.join("w.h"), "#define W 1\n").unwrap();
    let shard = tree.verify_task(&n_cpp, good, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    let w_h = first.join("w.h");
    assert_eq!(
        reason(&shard),
        format!("pre-image: {} appeared since the map phase", w_h.display())
    );
}

// spec/verify.md §5.1 "Loading": a blob of the right digest but not of the
// size the overlay gives fails the job.
#[test]
fn s5_1_a_blob_of_another_size_fails_the_job() {
    let tree = Tree::new("size");
    let (o, _, declared) = tree.map_c();
    let mut overlay = tree.include_overlay(&[]);
    let v = overlay.find(&tree.path("v.h")).unwrap();
    overlay.files[v].post.size += 1;
    let ovly = tree.put_overlay(&overlay, &[C_POST.as_bytes(), V_POST.as_bytes()]);
    let e = tree.verify_task(&o, ovly, declared).unwrap_err();
    assert!(e.contains(&format!("has {} bytes; the overlay {ovly} gives", V_POST.len())), "{e}");
}

// WP-074 review F1, spec/verify.md §5.1 "Loading": an overlay two of whose
// entries are one file gives it two post-images; the job fails, naming
// both, whatever the observer reads. Hard links `m.h` and `m2.h`; `q.cpp`
// reads `m2.h`. With `m.h` alone the observer is not verified (another name
// of the overlay's file, F3); with both entries the job fails. Then the
// same with two spellings of `w.h` on a volume that ignores case (skipped
// on one that does not).
#[test]
fn s5_1_two_entries_of_one_file_fail_the_job() {
    let tree = Tree::new("two-entries");
    let dir = tree.subject.clone();
    let m1: &[u8] = b"#define M 1\n";
    let m2: &[u8] = b"#define M 2\n";
    std::fs::write(dir.join("m.h"), m1).unwrap();
    // Two names of one file (identity); neither is replaced: the post-image
    // is the overlay's (spec/harness.md §11.4).
    std::fs::hard_link(dir.join("m.h"), dir.join("m2.h")).unwrap();
    std::fs::write(dir.join("q.cpp"), "#include \"m2.h\"\nint q = M;\n").unwrap();
    let (q_cpp, declared) = tree.map_publish("q.cpp", 8);
    let only = Overlay {
        edits: Digest([0x0f; 32]),
        files: vec![tree.entry("m.h", m1, m2)],
        spellings: Vec::new(),
    };
    let only = tree.put_overlay(&only, &[m2]);
    let shard = tree.verify_limit(&q_cpp, only, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    let both = Overlay {
        edits: Digest([0x0f; 32]),
        files: vec![tree.entry("m.h", m1, m2), tree.entry("m2.h", m1, m1)],
        spellings: Vec::new(),
    };
    let both = tree.put_overlay(&both, &[m2, m1]);
    let err = tree.verify_limit(&q_cpp, both, declared, None).unwrap_err();
    let (first_name, second_name) = (dir.join("m.h"), dir.join("m2.h"));
    assert_eq!(
        err,
        format!(
            "the overlay {both} names one file twice: {} and {} are the same file",
            first_name.display(),
            second_name.display()
        )
    );
    // An observer that reads neither: the refusal does not depend on what
    // the observer read.
    let err = tree.verify_limit(&tree.observer("e.cpp"), both, declared, None).unwrap_err();
    assert!(err.contains("names one file twice"), "{err}");

    let upper = dir.join("W.h");
    let lower = std::fs::metadata(dir.join("w.h")).unwrap();
    let same = std::fs::metadata(&upper)
        .is_ok_and(|meta| (meta.dev(), meta.ino()) == (lower.dev(), lower.ino()));
    if !same {
        eprintln!("the volume tells W.h from w.h: the case half is skipped");
        return;
    }
    std::fs::write(dir.join("k.cpp"), "#include \"W.h\"\nint k = W;\n").unwrap();
    let (k_cpp, declared) = tree.map_publish("k.cpp", 9);
    let w_h = std::fs::read(dir.join("w.h")).unwrap();
    let w3: &[u8] = b"#define W 3\n";
    let mut files = vec![tree.entry("W.h", &w_h, &w_h), tree.entry("w.h", &w_h, w3)];
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let both = Overlay { edits: Digest([0x0f; 32]), files, spellings: Vec::new() };
    let both = tree.put_overlay(&both, &[&w_h, w3]);
    let err = tree.verify_limit(&k_cpp, both, declared, None).unwrap_err();
    assert!(err.contains("names one file twice"), "{err}");
}

// WP-074 review F3: an input that is an overlay file by identity and not by
// path (another name of it) is not blamed on drift.
#[test]
fn s5_3_another_name_of_an_overlay_file() {
    let tree = Tree::new("alias");
    let s = tree.subject.clone();
    let m1: &[u8] = b"#define M 1\n";
    let m2: &[u8] = b"#define M 2\n";
    std::fs::write(s.join("m.h"), m1).unwrap();
    // Two names of one file (identity); neither is replaced: the post-image
    // is the overlay's (spec/harness.md §11.4).
    std::fs::hard_link(s.join("m.h"), s.join("m2.h")).unwrap();
    std::fs::write(s.join("q.cpp"), "#include \"m2.h\"\nint q = M;\n").unwrap();
    let (o, declared) = tree.map_publish("q.cpp", 8);
    let only = Overlay {
        edits: Digest([0x0f; 32]),
        files: vec![tree.entry("m.h", m1, m2)],
        spellings: Vec::new(),
    };
    let only = tree.put_overlay(&only, &[m2]);
    let shard = tree.verify_limit(&o, only, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&shard),
        format!(
            "pre-image: {} is another name of the overlay's {}",
            s.join("m2.h").display(),
            s.join("m.h").display()
        )
    );
}

// WP-074 review F4: an overlay file the pre shard lists that is gone from
// disk since the map phase is a fact about the subject, not a failure of
// the run: it is never given to the front end. Read by the post-image, the
// parse cannot find it (`not_parsed`, as in the tree form); not read, the
// observer is `unverifiable` (O2's second part).
#[test]
fn s5_1_an_overlay_file_gone_since_the_map_phase() {
    let tree = Tree::new("gone");
    let (o, _, declared) = tree.map_c();
    let w = std::fs::read(tree.subject.join("w.h")).unwrap();
    let w2: &[u8] = b"#define W 1\n\n";
    let read = tree.put_overlay(
        &tree.include_overlay(&[tree.entry("w.h", &w, w2)]),
        &[C_POST.as_bytes(), V_POST.as_bytes(), w2],
    );
    let c_pre = std::fs::read(tree.subject.join("c.cpp")).unwrap();
    let c_alone: &[u8] = b"int c = 1;\n";
    let mut files = vec![tree.entry("c.cpp", &c_pre, c_alone), tree.entry("w.h", &w, w2)];
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let not_read = Overlay { edits: Digest([0x0f; 32]), files, spellings: Vec::new() };
    let not_read = tree.put_overlay(&not_read, &[c_alone, w2]);
    let shard = tree.verify_limit(&o, not_read, declared, None).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    assert_ne!(kind(&shard), VerdictKind::Unverifiable, "{}", reason(&shard));

    std::fs::remove_file(tree.subject.join("w.h")).unwrap();
    let shard = tree.verify_limit(&o, read, declared, None).unwrap();
    assert_eq!(shard.outcome.status, Status::Diagnosed, "{:?}", shard.outcome);
    let shard = tree.verify_limit(&o, not_read, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&shard),
        format!(
            "pre-image: {} is gone since the map phase (run `slip parse` again)",
            tree.subject.join("w.h").display()
        )
    );
}

// WP-074 review F5, spec/model.md §5.4: a request with an ignorable section
// a reader skips. The request id is the content id of the stored bytes,
// and the edits name it.
#[test]
fn s8_1_a_request_with_an_ignorable_section() {
    use slip_model::json::Value;
    let tree = Tree::new("ignorable");
    let (t, plain) = tree.rename();
    let text = slip_model::formats::dump(&t.to_bytes()).unwrap();
    let mut v = slip_model::json::parse(text.as_bytes()).unwrap();
    let Value::Object(top) = &mut v else { panic!("a container is an object") };
    let Some((_, Value::Object(sections))) = top.iter_mut().find(|(k, _)| k == "sections") else {
        panic!("a container has sections")
    };
    sections.push(("#64".into(), Value::Object(vec![("hex".into(), Value::Str("00".into()))])));
    let bytes = slip_model::formats::undump(&v).unwrap();
    assert_ne!(bytes, t.to_bytes());
    let id = tree.store.publish_object(Object::Transform, &bytes).unwrap();
    assert_ne!(id, plain);
    let shard = tree.map(&tree.observer("a.cpp"), Some(id)).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    assert_eq!(shard.edits.as_ref().unwrap().transform, id);
}

// WP-074 review F2, spec/verify.md §5.3 N2 in the overlay form: a found
// place (`lnk/l.h`) that leads, since the map phase, to another file of the
// same bytes. The file the map parse read is still there and unchanged (no
// other guard sees it); only N2 does.
#[test]
fn s5_3_n2_alone() {
    let tree = Tree::new("n2");
    let s = tree.subject.clone();
    std::fs::create_dir_all(s.join("d1")).unwrap();
    std::fs::create_dir_all(s.join("d2")).unwrap();
    std::fs::write(s.join("d1/l.h"), "#define L 1\n").unwrap();
    std::fs::write(s.join("d2/l.h"), "#define L 1\n").unwrap();
    std::os::unix::fs::symlink("d1", s.join("lnk")).unwrap();
    std::fs::write(s.join("l.cpp"), "#include \"lnk/l.h\"\nint l = L;\n").unwrap();
    let (o, declared) = tree.map_publish("l.cpp", 7);
    let ovly = tree.put_overlay(&tree.include_overlay(&[]), &[]);
    let shard = tree.verify_limit(&o, ovly, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    std::fs::remove_file(s.join("lnk")).unwrap();
    std::os::unix::fs::symlink("d2", s.join("lnk")).unwrap();
    let shard = tree.verify_limit(&o, ovly, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    assert!(reason(&shard).contains("now leads to"), "{}", reason(&shard));
}

// spec/verify.md §5.3 O2's second part in the overlay form (review R6): the
// edit drops `c.cpp`'s `#include "w.h"`; `w.h` then changes on disk. The
// post-image does not read it; only `left_behind` sees it.
#[test]
fn s5_3_left_behind_in_the_overlay_form() {
    let tree = Tree::new("left");
    let (o, _, declared) = tree.map_c();
    let c_pre = std::fs::read(tree.subject.join("c.cpp")).unwrap();
    let c_post: &[u8] = b"#define W 1\nint c = W;\n";
    let overlay = Overlay {
        edits: Digest([0x0f; 32]),
        files: vec![tree.entry("c.cpp", &c_pre, c_post)],
        spellings: Vec::new(),
    };
    let ovly = tree.put_overlay(&overlay, &[c_post]);
    let shard = tree.verify_limit(&o, ovly, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    std::fs::write(tree.subject.join("w.h"), "#define W 2\n").unwrap();
    let shard = tree.verify_limit(&o, ovly, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&shard),
        format!(
            "pre-image: {} changed since the map phase (run `slip parse` again)",
            tree.subject.join("w.h").display()
        )
    );
}

// spec/verify.md §5.3 G4 in the overlay form (review R4): a command that
// names a response file.
#[test]
fn s5_3_g4_in_the_overlay_form() {
    let tree = Tree::new("g4");
    let s = tree.subject.clone();
    std::fs::write(s.join("flags.rsp"), "-DQ=1\n").unwrap();
    std::fs::write(s.join("r.cpp"), "int r = Q;\n").unwrap();
    let mut o = observer_of(&s, "r.cpp", 11);
    let word = format!("@{}", s.join("flags.rsp").display());
    o.command.argv.insert(2, word.as_bytes().to_vec());
    o.id = o.command.observer_id();
    let shard = tree.map(&o, None).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    tree.store.publish_shard(&tree.plan_id, Phase::Map, ROUND, &o.id, &shard.to_bytes()).unwrap();
    let declared = tree.store.publish_declared(&Declared::none().to_bytes()).unwrap();
    let ovly = tree.put_overlay(&tree.include_overlay(&[]), &[]);
    let shard = tree.verify_limit(&o, ovly, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&shard),
        format!(
            "post-image: the command names the response file {word}, whose words slip cannot examine"
        )
    );
}

// spec/verify.md §5.1 "Loading" (review R14): the first parse has an error
// (the disk's `z.h`, which the edit newly includes) that the overlay's
// `z.h` fixes: the loop gives it and parses again although the parse was
// diagnosed.
#[test]
fn s5_1_a_diagnosed_parse_that_loading_fixes() {
    let tree = Tree::new("diagnosed");
    std::fs::write(tree.subject.join("z.h"), "int zz = nope;\n").unwrap();
    let (o, _, declared) = tree.map_c();
    let c_pre = std::fs::read(tree.subject.join("c.cpp")).unwrap();
    let c_post: &[u8] = b"#include \"w.h\"\n#include \"z.h\"\nint c = Z;\n";
    let z_post: &[u8] = b"#define Z 1\n";
    let mut files =
        vec![tree.entry("c.cpp", &c_pre, c_post), tree.entry("z.h", b"int zz = nope;\n", z_post)];
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let overlay = Overlay { edits: Digest([0x0f; 32]), files, spellings: Vec::new() };
    let ovly = tree.put_overlay(&overlay, &[c_post, z_post]);
    let shard = tree.verify_limit(&o, ovly, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
}

// spec/verify.md §5.1 "Loading" (review R2): the edit embeds `y.bin`, an
// overlay file the map parse did not read and whose disk bytes are not the
// overlay's. The loop gives it; stopped after one parse, O1.
#[test]
fn s5_1_embed_of_an_overlay_file() {
    let tree = Tree::new("embed");
    let s = tree.subject.clone();
    std::fs::write(s.join("x.bin"), "AB").unwrap();
    std::fs::write(s.join("y.bin"), "ZZ").unwrap();
    let pre: &[u8] = b"const char d[] = {\n#embed \"x.bin\"\n};\n";
    let post: &[u8] = b"const char d[] = {\n#embed \"y.bin\"\n};\n";
    std::fs::write(s.join("em.cpp"), pre).unwrap();
    let (o, declared) = tree.map_publish("em.cpp", 10);
    let mut files = vec![tree.entry("em.cpp", pre, post), tree.entry("y.bin", b"ZZ", b"AB")];
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let overlay = Overlay { edits: Digest([0x0f; 32]), files, spellings: Vec::new() };
    let ovly = tree.put_overlay(&overlay, &[post, b"AB"]);
    let shard = tree.verify_limit(&o, ovly, declared, None).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    let y = tree.path("y.bin");
    let input = shard.inputs.as_ref().unwrap().iter().find(|f| f.path == y).expect("y.bin");
    assert_eq!(input.content, content_digest(b"AB"));
    let stopped = tree.verify_limit(&o, ovly, declared, Some(1)).unwrap();
    assert_eq!(kind(&stopped), VerdictKind::Unverifiable);
    assert_eq!(
        reason(&stopped),
        format!("post-image: {} was not read through the overlay", s.join("y.bin").display())
    );
}

// spec/verify.md §10.4 (review R17): the obligations checked are those of
// the overlay files the post-image read, not of those given. `u.cpp`
// includes `a.cpp` before the edit and not after: `a.cpp` (whose call of
// `f` is an obligation) is given and not read, so nothing is seen.
#[test]
fn s10_4_obligations_of_the_files_read() {
    let tree = Tree::new("obligations-read");
    let r = tree.map_and_reduce();
    let built = slip_apply::build(&r.set, r.set_id, &Disk).unwrap();
    let mut overlay = built.overlay.clone();
    assert!(!overlay.spellings.is_empty());
    let u_pre: &[u8] = b"#include \"a.cpp\"\nint u = x;\n";
    let u_post: &[u8] = b"int u = 0;\n";
    std::fs::write(tree.subject.join("u.cpp"), u_pre).unwrap();
    let (o, _) = tree.map_publish("u.cpp", 12);
    overlay.files.push(tree.entry("u.cpp", u_pre, u_post));
    assert!(
        overlay.files.windows(2).all(|w| w[0].path < w[1].path),
        "u.cpp sorts last: no obligation's file index moves"
    );
    let mut blobs: Vec<&[u8]> = built.blobs.iter().map(|(_, b)| b.as_slice()).collect();
    blobs.push(u_post);
    let ovly = tree.put_overlay(&overlay, &blobs);
    let shard = tree.verify_limit(&o, ovly, r.declared_id, None).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    let spellings = shard.spellings.as_ref().expect("the overlay has obligations");
    assert_eq!(spellings.seen, 0, "{spellings:?}");
    assert_eq!(counter(&shard, "verify.spelling.obligations"), Some(0));
}

// ----- WP-151: the request's and the declared files' entries; one file once ---

/// `s/dep/x.h` and `s/café/y.h` (NFC): directories a request may carve
/// out read-only, which the rename of `f` does not touch.
fn with_dependencies(tree: &Tree) {
    for (dir, file) in [("dep", "x.h"), ("caf\u{e9}", "y.h")] {
        std::fs::create_dir_all(tree.subject.join(dir)).unwrap();
        std::fs::write(tree.subject.join(dir).join(file), "int x;\n").unwrap();
    }
}

/// The rename of `f` to `g` with `readonly` carved out, published as a
/// hand-written request (the product's writer, then altered).
fn request_with_readonly(tree: &Tree, readonly: &str) -> (Transform, Digest) {
    let (mut t, _) = tree.rename();
    t.ownership.readonly = vec![bytes(&tree.subject.join(readonly))];
    let id = tree.store.publish_object(Object::Transform, &t.to_bytes()).unwrap();
    (t, id)
}

/// The line that names a read-only entry `asked` and why it is refused.
fn entry_line(tree: &Tree, asked: &str, why: &str) -> String {
    format!("ownership: the readonly entry {} {why}", tree.subject.join(asked).display())
}

/// `spec/edits.md` §2.1, §8.1 (WP-151 item 1, row T1): the map job reads
/// the request and checks that every ownership entry exists and is
/// spelled on disk before anything is parsed; an entry that does not
/// exist fails the job (a failure of the run, never a shard), naming it.
/// The control: the entry as on disk maps. Any volume.
#[test]
fn s8_1_the_map_job_refuses_an_entry_that_does_not_exist() {
    let tree = Tree::new("t1-map-missing");
    with_dependencies(&tree);
    let o = tree.observer("a.cpp");
    let (_, id) = request_with_readonly(&tree, "nope");
    let e = tree.map(&o, Some(id)).unwrap_err();
    assert!(e.contains(&entry_line(&tree, "nope", "does not exist")), "{e}");
    let (_, id) = request_with_readonly(&tree, "dep");
    let shard = tree.map(&o, Some(id)).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
}

/// The same for an entry in another case (`s/DEP` for `s/dep`) or another
/// Unicode normalisation (`s/café` in NFD for the NFC name on disk): the
/// job fails naming the entry and its spelling on disk. On the folding
/// root; skipped on a volume that does not fold.
#[test]
fn s8_1_the_map_job_refuses_an_entry_not_spelled_on_disk() {
    let tree = Tree::new_in(&common::folding_root(), "t1-map-case");
    with_dependencies(&tree);
    if std::fs::metadata(tree.subject.join("DEP")).is_err() {
        eprintln!("skipped: the folding root's volume is case-sensitive: `DEP` names nothing");
        return;
    }
    let o = tree.observer("a.cpp");
    for (asked, disk) in [("DEP", "dep"), ("cafe\u{301}", "caf\u{e9}")] {
        let (_, id) = request_with_readonly(&tree, asked);
        let e = tree.map(&o, Some(id)).unwrap_err();
        let why = format!("is not spelled as on disk ({})", tree.subject.join(disk).display());
        assert!(e.contains(&entry_line(&tree, asked, &why)), "{e}");
    }
}

/// `spec/exec.md` §8.6, `spec/verify.md` §2.1 (WP-151 item 1): the verify
/// job reads the declared changes and checks their `files` as a request's
/// ownership: an entry that does not exist, or (on a volume that folds)
/// is in another case, fails the job naming it. The control: the entries
/// as on disk verify.
#[test]
fn s5_1_the_verify_job_checks_the_declared_files() {
    for root in [common::work_root(), common::folding_root()] {
        let tree = Tree::new_in(&root, "t1-verify");
        with_dependencies(&tree);
        let r = tree.map_and_reduce();
        let (overlay, _) = tree.publish_overlay(&r.set, r.set_id);
        let o = tree.observer("a.cpp");
        let with = |readonly: &str| {
            let mut files = tree.ownership();
            files.readonly = vec![bytes(&tree.subject.join(readonly))];
            let d = r.declared.clone().with_files(Some(files));
            tree.store.publish_declared(&d.to_bytes()).unwrap()
        };
        let shard = tree.verify_task(&o, overlay, with("dep")).unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
        let e = tree.verify_task(&o, overlay, with("nope")).unwrap_err();
        assert!(e.contains(&entry_line(&tree, "nope", "does not exist")), "{e}");
        if std::fs::metadata(tree.subject.join("DEP")).is_ok() {
            let e = tree.verify_task(&o, overlay, with("DEP")).unwrap_err();
            let why = format!("is not spelled as on disk ({})", tree.subject.join("dep").display());
            assert!(e.contains(&entry_line(&tree, "DEP", &why)), "{e}");
        } else {
            eprintln!("{}: case-sensitive: the case of `DEP` not tried here", root.display());
        }
    }
}

/// `spec/edits.md` §7.1 (WP-151 item 3; WP-074's second review, F1): two
/// hard links to one file, each edited by an observer that includes it
/// under its own name (`a.cpp` reads `h.h`, `b.cpp` reads `d.h`, now a link
/// of `h.h`), are refused where the overlay is made: reduce's edit set
/// names both, and `reduce_round` refuses it, naming both paths, before it
/// publishes anything. Under the work root (`$SLIP_TEST_ROOT`, the
/// container's disk), where hard links are honest; never on the mount.
#[test]
fn s7_1_two_hard_links_edited_under_their_names_refused_by_reduce() {
    let tree = Tree::new("hard-links-reduce");
    let (h, d) = (tree.subject.join("h.h"), tree.subject.join("d.h"));
    std::fs::remove_file(&d).unwrap();
    std::fs::hard_link(&h, &d).unwrap();
    let (t, id) = tree.rename();
    for o in &tree.plan.observers {
        let shard = tree.map(o, Some(id)).unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
        tree.store
            .publish_shard(&tree.plan_id, Phase::Map, ROUND, &o.id, &shard.to_bytes())
            .unwrap();
    }
    let selected: Vec<usize> = (0..tree.plan.observers.len()).collect();
    let sets = tree.store.object_path(Object::EditSet, &Digest([0; 32]));
    let sets = sets.parent().unwrap();
    let before = std::fs::read_dir(sets).map_or(0, Iterator::count);
    let e = reduce_round(&tree.store, tree.plan_id, &tree.plan, ROUND, &selected, (id, &t), 1)
        .unwrap_err()
        .to_string();
    let both = format!("{} and {} are one file", d.display(), h.display());
    assert!(e.contains(&both), "{e}");
    let after = std::fs::read_dir(sets).map_or(0, Iterator::count);
    assert_eq!(after, before, "nothing published");
}

/// The same refusal by the overlay's builder (`slip_apply::build_into`),
/// for an edit set made while the two were two files: `d.h` made a link of
/// `h.h` after reduce. Refused naming both paths before the sink (which
/// publishes the post-images as blobs) is called once; the files' bytes
/// are not read first (it is not `stale_file`). Under the work root, as
/// above.
#[test]
fn s7_1_two_hard_links_refused_by_the_overlay_builder() {
    let tree = Tree::new("hard-links-build");
    let reduced = tree.map_and_reduce();
    let (h, d) = (tree.subject.join("h.h"), tree.subject.join("d.h"));
    let paths: Vec<Vec<u8>> = reduced.set.files.iter().map(|f| f.path.clone()).collect();
    assert!(paths.contains(&bytes(&h)) && paths.contains(&bytes(&d)), "{paths:?}");
    std::fs::remove_file(&d).unwrap();
    std::fs::hard_link(&h, &d).unwrap();
    let mut sunk = 0;
    let err = slip_apply::build_into(
        &reduced.set,
        reduced.set_id,
        &slip_cli::transform::store::DiskFiles,
        &mut |_, _| {
            sunk += 1;
            Ok(())
        },
    )
    .unwrap_err();
    let both = format!("{} and {} are one file", d.display(), h.display());
    assert!(matches!(&err, slip_apply::ApplyError::Invalid(m) if m.contains(&both)), "{err:?}");
    assert_eq!(sunk, 0, "nothing handed to the sink");
}
