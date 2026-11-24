//! `docs/wp/WP-074.md` "Tests": the worker's two tasks of a transform's
//! run, in process, with no coordinator. The map task with a transform
//! (`spec/edits.md` §8.1), reduce (`slip_cli::transform::reduce`), the
//! overlay built by hand with `slip_apply::build` (`spec/edits.md` §7.1),
//! and the verify task through it (`spec/verify.md` §5.1 "The overlay
//! form", §5.3 O1 and O2, §9.1, §10.4), over a small tree and a store in a
//! directory of their own under the target directory.
//!
//! A parse changes the process's current directory: every test that
//! parses holds [`LOCK`], and every path is absolute.

use std::os::unix::ffi::OsStrExt;
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
        static N: AtomicUsize = AtomicUsize::new(0);
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp074-{name}-{}-{n}", std::process::id()));
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
// has `spellings`, with rules or without; its `binding`, when present, has
// no overlay.
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
        if let Some(b) = shard.binding {
            assert_eq!(b.overlay, None);
        }
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
}

// WP-074's guards of the overlay form (spec/landed/exec/WP-074.md §5): an
// overlay made from another `c.cpp` than the map phase read (the subject's
// sums), and a header that appeared, after the map phase, where the map
// parse looked for one and found none (N1 over every negative input).
#[test]
fn s5_3_subject_sums_and_n1() {
    let tree = Tree::new("sums");
    let (o, _, declared) = tree.map_c();
    let mut overlay = tree.include_overlay(&[]);
    let c = overlay.find(&tree.path("c.cpp")).unwrap();
    overlay.files[c].pre = content(b"int c = 1;\n");
    let ovly = tree.put_overlay(&overlay, &[C_POST.as_bytes(), V_POST.as_bytes()]);
    let shard = tree.verify_task(&o, ovly, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    let c = String::from_utf8(tree.path("c.cpp")).unwrap();
    assert_eq!(
        reason(&shard),
        format!("pre-image: the overlay was made from another {c} than the map phase read")
    );

    // `n.cpp` includes `<w.h>` searching `i/` before the subject: the map
    // parse looked for `i/w.h` and found none.
    let i = tree.base.join("i");
    std::fs::create_dir_all(&i).unwrap();
    std::fs::write(tree.subject.join("n.cpp"), "#include <w.h>\nint n = W;\n").unwrap();
    let mut n = observer_of(&tree.subject, "n.cpp", 5);
    let flags = [format!("-I{}", i.display()), format!("-I{}", tree.subject.display())];
    n.command.argv.splice(2..2, flags.iter().map(|f| f.as_bytes().to_vec()));
    n.id = n.command.observer_id();
    let shard = tree.map(&n, None).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    tree.store.publish_shard(&tree.plan_id, Phase::Map, ROUND, &n.id, &shard.to_bytes()).unwrap();
    let good = tree.put_overlay(&tree.include_overlay(&[]), &[]);
    let shard = tree.verify_task(&n, good, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Verified, "{}", reason(&shard));
    std::fs::write(i.join("w.h"), "#define W 1\n").unwrap();
    let shard = tree.verify_task(&n, good, declared).unwrap();
    assert_eq!(kind(&shard), VerdictKind::Unverifiable);
    let w = i.join("w.h");
    assert_eq!(reason(&shard), format!("pre-image: {} appeared since the map phase", w.display()));
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
