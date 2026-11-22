//! `spec/exec.md` §2, §2.2 and `spec/edits.md` §10.4: the store's files of a
//! transform's run. The objects (`transforms/`, `edits/`, `overlays/`,
//! `results/`), the blobs and their check, the next round, the transform
//! run records beside the phases', and the journal's refusal. No child
//! process is started.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use slip_exec::store::{Access, Object, Store, StoreError};
use slip_model::container::Writer;
use slip_model::hash::content_digest;
use slip_model::wire::Encoder;
use slip_model::{Digest, Phase, Run, RunRecord};

// ---------------------------------------------------------------------------
// Helpers

struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let d =
        std::env::temp_dir().join(format!("slip-exec-objects-{}-{tag}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    Scratch(d)
}

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

const ALL: [Object; 4] = [Object::Transform, Object::EditSet, Object::Overlay, Object::Result];

/// A container of `kind`, version 1, with one section holding `payload`.
fn container(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut w = Writer::new(kind, 1);
    w.section(1, payload);
    w.finish()
}

fn exclusive(dir: &Path) -> Store {
    Store::open(&dir.join("store"), Access::Exclusive).unwrap()
}

fn phase_run(plan: Digest, phase: Phase, round: u64, started: u64) -> Vec<u8> {
    RunRecord {
        run: Run {
            tool_slip: "s".into(),
            tool_kiln: "k".into(),
            plan,
            phase,
            round,
            started_unix_ns: started,
            wall_ns: 1,
            cores: 1,
            workers_max: 1,
            memory_budget: 0,
            peak_rss_sum: 0,
        },
        jobs: vec![],
    }
    .to_bytes()
}

/// A `RUNT` container (`spec/edits.md` §10.6) with no steps.
fn transform_run(plan: Digest, round: u64, started: u64) -> Vec<u8> {
    let mut e = Encoder::new();
    e.str("s").str("k").digest(&plan).uv(round).digest(&d(0x77));
    e.u8(1).digest(&d(0x78)); // edits
    e.u8(0); // declared
    e.u8(1).digest(&d(0x79)); // overlay
    e.u8(0); // result
    e.uv(started).uv(5).count(0);
    container(*b"RUNT", e.as_bytes())
}

/// Identity of a file on disk: inode and modification time.
fn identity(path: &Path) -> (u64, i64, i64) {
    let m = fs::metadata(path).unwrap();
    (m.ino(), m.mtime(), m.mtime_nsec())
}

// ---------------------------------------------------------------------------
// Objects

/// `spec/edits.md` §10.4, `spec/exec.md` §2.2: each object kind is
/// published under its content id in its directory (made by the first
/// publish), read back, and found by prefix: none, one, two matches.
/// Publishing it again leaves the file untouched.
#[test]
fn s10_4_objects_published_read_and_found() {
    let dir = scratch("objects");
    let store = exclusive(&dir);
    let names = [("transforms", "trfm", *b"TRFM"), ("edits", "edts", *b"EDTS")];
    let names = [names[0], names[1], ("overlays", "ovly", *b"OVLY"), ("results", "rslt", *b"RSLT")];
    for (object, (dir_name, ext, kind)) in ALL.into_iter().zip(names) {
        assert_eq!((object.dir(), object.extension(), object.kind()), (dir_name, ext, kind));
        // A store initialised before the directory existed.
        assert!(!store.root().join(dir_name).exists());
        assert_eq!(store.find_object(object, "").unwrap(), vec![]);
        // Seventeen containers: two of them share a first digit.
        let mut ids = Vec::new();
        for i in 0..17u8 {
            let bytes = container(kind, &[i]);
            let id = store.publish_object(object, &bytes).unwrap();
            assert_eq!(id, content_digest(&bytes[..bytes.len() - 32]));
            let path = store.object_path(object, &id);
            assert_eq!(path, store.root().join(dir_name).join(format!("{id}.{ext}")));
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(store.read_object(object, &id).unwrap(), Some(bytes.clone()));
            ids.push(id);
        }
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(store.find_object(object, "").unwrap(), sorted, "the empty prefix: all");
        let hex: Vec<String> = ids.iter().map(Digest::to_string).collect();
        // One match: a full id.
        assert_eq!(store.find_object(object, &hex[0]).unwrap(), vec![ids[0]]);
        // Two matches: a one- or two-digit prefix that exactly two ids
        // share (the ids are content digests, so which one is fixed).
        let prefix = (0..16 * 17)
            .map(|n| if n < 16 { format!("{n:x}") } else { format!("{:02x}", n - 16) })
            .find(|p| hex.iter().filter(|h| h.starts_with(p.as_str())).count() == 2)
            .expect("a prefix shared by exactly two ids");
        let want: Vec<Digest> =
            sorted.iter().filter(|id| id.to_string().starts_with(&prefix)).copied().collect();
        assert_eq!(want.len(), 2);
        assert_eq!(store.find_object(object, &prefix).unwrap(), want);
        // None: an id not published, a prefix that is not lower-case hex,
        // one longer than 64 digits.
        let absent = d(0xee).to_string();
        assert!(!hex.contains(&absent));
        assert_eq!(store.find_object(object, &absent).unwrap(), vec![]);
        assert_eq!(store.find_object(object, &hex[0].to_uppercase()).unwrap(), vec![]);
        assert_eq!(store.find_object(object, &format!("{}0", hex[0])).unwrap(), vec![]);
        assert_eq!(store.read_object(object, &d(0xee)).unwrap(), None);
        // Again: not an error, the file untouched.
        let path = store.object_path(object, &ids[0]);
        let before = identity(&path);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let again = store.publish_object(object, &container(kind, &[0])).unwrap();
        assert_eq!(again, ids[0]);
        assert_eq!(identity(&path), before);
    }
    // Each kind is found only in its own directory.
    let trfm = store.publish_object(Object::Transform, &container(*b"TRFM", b"x")).unwrap();
    assert_eq!(store.read_object(Object::EditSet, &trfm).unwrap(), None);
    // Nothing is left under tmp/.
    assert_eq!(fs::read_dir(store.root().join("tmp")).unwrap().count(), 0);
}

/// `spec/exec.md` §2.1 "Publishing validates": a container of another
/// kind, a file that is not a container, a broken trailer and an unknown
/// version are `Invalid`, and nothing is written.
#[test]
fn s2_1_object_of_another_kind_refused() {
    let dir = scratch("refused");
    let store = exclusive(&dir);
    let trfm = container(*b"TRFM", b"x");
    let mut corrupt = trfm.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    let mut v2 = Writer::new(*b"EDTS", 2);
    v2.section(1, b"x");
    let cases: Vec<(Object, Vec<u8>)> = vec![
        (Object::EditSet, trfm.clone()),
        (Object::Overlay, trfm.clone()),
        (Object::Result, container(*b"SHRD", b"x")),
        (Object::Transform, b"not a container".to_vec()),
        (Object::Transform, corrupt),
        (Object::EditSet, v2.finish()),
        (Object::Transform, Vec::new()),
    ];
    for (object, bytes) in cases {
        match store.publish_object(object, &bytes) {
            Err(StoreError::Invalid(m)) => {
                let kind = String::from_utf8_lossy(&object.kind()).into_owned();
                assert!(m.contains(&kind), "{m}");
            }
            other => panic!("{object:?}: {other:?}"),
        }
        assert!(!store.root().join(object.dir()).exists(), "{object:?}");
    }
    assert_eq!(fs::read_dir(store.root().join("tmp")).unwrap().count(), 0);
}

// ---------------------------------------------------------------------------
// Blobs

/// `spec/edits.md` §10.4: a blob is the bytes under their content digest,
/// read back; publishing it again leaves it untouched; an absent one is
/// `None`.
#[test]
fn s10_4_blob_read_back() {
    let dir = scratch("blob");
    let store = exclusive(&dir);
    assert!(!store.root().join("blobs").exists());
    for bytes in [&b"int main() {}\n"[..], b"", &[0xff, 0, 0x80]] {
        let id = store.publish_blob(bytes).unwrap();
        assert_eq!(id, content_digest(bytes));
        assert_eq!(store.blob_path(&id), store.root().join("blobs").join(id.to_string()));
        assert_eq!(fs::read(store.blob_path(&id)).unwrap(), bytes);
        assert_eq!(store.read_blob(&id).unwrap().as_deref(), Some(bytes));
        let before = identity(&store.blob_path(&id));
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(store.publish_blob(bytes).unwrap(), id);
        assert_eq!(identity(&store.blob_path(&id)), before);
    }
    assert_eq!(store.read_blob(&d(1)).unwrap(), None);
}

/// `spec/edits.md` §10.4: a blob that does not hash to its name (its bytes
/// changed on disk) is refused when read.
#[test]
fn s10_4_changed_blob_refused() {
    let dir = scratch("blob-changed");
    let store = exclusive(&dir);
    let id = store.publish_blob(b"pre-image").unwrap();
    fs::write(store.blob_path(&id), b"pre-imagE").unwrap();
    match store.read_blob(&id) {
        Err(StoreError::Invalid(m)) => assert!(m.contains(&id.to_string()), "{m}"),
        other => panic!("{other:?}"),
    }
    // Truncated to nothing: refused too.
    fs::write(store.blob_path(&id), b"").unwrap();
    assert!(matches!(store.read_blob(&id), Err(StoreError::Invalid(_))));
}

// ---------------------------------------------------------------------------
// Rounds

/// `spec/exec.md` §2, `spec/edits.md` §8: the next round is the least that
/// has no shard directory of either phase and no transform run record.
#[test]
fn s2_next_round() {
    let dir = scratch("rounds");
    let store = exclusive(&dir);
    let plan = d(0x50);
    let shards = store.root().join("shards").join(plan.to_string());
    // No directory at all.
    assert_eq!(store.next_round(&plan).unwrap(), 0);
    fs::create_dir_all(shards.join("map.0")).unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 1);
    fs::create_dir_all(shards.join("verify.1")).unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 2);
    // Names that are not rounds are ignored.
    for name in ["map.02", "verify.x", "other.2", "map2"] {
        fs::create_dir_all(shards.join(name)).unwrap();
    }
    assert_eq!(store.next_round(&plan).unwrap(), 2);
    // Another plan's rounds are not this plan's.
    assert_eq!(store.next_round(&d(0x51)).unwrap(), 0);
    // A transform run record of round 2, no shard directory of round 2.
    store.publish_transform_run(&plan, 2, 10, &transform_run(plan, 2, 10)).unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 3);
    // A phase's run record does not take a round: only shard directories
    // and transform run records do.
    store.publish_run(&plan, Phase::Map, 3, 11, &phase_run(plan, Phase::Map, 3, 11)).unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 3);

    // A transform run record of round 2 alone: the least free round is 0.
    let other = d(0x52);
    store.publish_transform_run(&other, 2, 10, &transform_run(other, 2, 10)).unwrap();
    assert_eq!(store.next_round(&other).unwrap(), 0);
}

/// `spec/exec.md` §2.2: a round is a transform's when it has a transform
/// run record; a phase's run record or shard directory does not make it one.
#[test]
fn s2_is_transform_round() {
    let dir = scratch("transform-round");
    let store = exclusive(&dir);
    let plan = d(0x60);
    assert!(!store.is_transform_round(&plan, 0).unwrap());
    store.publish_run(&plan, Phase::Map, 0, 5, &phase_run(plan, Phase::Map, 0, 5)).unwrap();
    fs::create_dir_all(store.root().join("shards").join(plan.to_string()).join("map.1")).unwrap();
    store.publish_transform_run(&plan, 1, 7, &transform_run(plan, 1, 7)).unwrap();
    assert!(!store.is_transform_round(&plan, 0).unwrap());
    assert!(store.is_transform_round(&plan, 1).unwrap());
    assert!(!store.is_transform_round(&plan, 2).unwrap());
    assert!(!store.is_transform_round(&d(0x61), 1).unwrap());
}

// ---------------------------------------------------------------------------
// Run records

/// `spec/exec.md` §2.2: `latest_run_of_round` is the greatest `ns` among
/// one round's records of a phase: two records of round 1, one of round 2.
#[test]
fn s2_latest_run_of_round() {
    let dir = scratch("latest-round");
    let store = exclusive(&dir);
    let plan = d(0x70);
    let a = phase_run(plan, Phase::Map, 1, 100);
    let b = phase_run(plan, Phase::Map, 1, 200);
    let c = phase_run(plan, Phase::Map, 2, 300);
    let v = phase_run(plan, Phase::Verify, 1, 400);
    for (round, ns, bytes) in [(1, 200, &b), (1, 100, &a), (2, 300, &c)] {
        store.publish_run(&plan, Phase::Map, round, ns, bytes).unwrap();
    }
    store.publish_run(&plan, Phase::Verify, 1, 400, &v).unwrap();
    assert_eq!(store.latest_run_of_round(&plan, Phase::Map, 1).unwrap(), Some(b));
    assert_eq!(store.latest_run_of_round(&plan, Phase::Map, 2).unwrap(), Some(c.clone()));
    assert_eq!(store.latest_run_of_round(&plan, Phase::Map, 3).unwrap(), None);
    assert_eq!(store.latest_run_of_round(&plan, Phase::Verify, 1).unwrap(), Some(v));
    assert_eq!(store.latest_run_of_round(&plan, Phase::Verify, 2).unwrap(), None);
    assert_eq!(store.latest_run(&plan, Phase::Map).unwrap(), Some(c));
}

/// `spec/exec.md` §2.2: neither `latest_run` nor `latest_run_of_round`
/// takes a transform run record for a phase's, even a later one.
#[test]
fn s2_latest_run_beside_a_transform_run_record() {
    let dir = scratch("latest-transform");
    let store = exclusive(&dir);
    let plan = d(0x71);
    store.publish_transform_run(&plan, 0, 900, &transform_run(plan, 0, 900)).unwrap();
    for phase in [Phase::Map, Phase::Verify] {
        assert_eq!(store.latest_run(&plan, phase).unwrap(), None);
        assert_eq!(store.latest_run_of_round(&plan, phase, 0).unwrap(), None);
    }
    let map = phase_run(plan, Phase::Map, 0, 100);
    store.publish_run(&plan, Phase::Map, 0, 100, &map).unwrap();
    assert_eq!(store.latest_run(&plan, Phase::Map).unwrap(), Some(map.clone()));
    assert_eq!(store.latest_run_of_round(&plan, Phase::Map, 0).unwrap(), Some(map));
    assert_eq!(store.latest_run(&plan, Phase::Verify).unwrap(), None);
}

/// `spec/edits.md` §10.4, §10.6: a transform run record is published at
/// `runs/<plan>/transform.<round>.<ns>.run`, and only under the plan, round
/// and start time it names; anything else is `Invalid`.
#[test]
fn s10_6_transform_run_record_path() {
    let dir = scratch("runt");
    let store = exclusive(&dir);
    let plan = d(0x80);
    let bytes = transform_run(plan, 4, 123);
    let path = store.publish_transform_run(&plan, 4, 123, &bytes).unwrap();
    let want = store.root().join("runs").join(plan.to_string()).join("transform.4.123.run");
    assert_eq!(path, want);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let refused: [(Digest, u64, u64, Vec<u8>); 5] = [
        (d(0x81), 4, 123, bytes.clone()),
        (plan, 5, 123, bytes.clone()),
        (plan, 4, 124, bytes.clone()),
        (plan, 4, 123, phase_run(plan, Phase::Map, 4, 123)),
        (plan, 4, 123, container(*b"RUNT", b"")),
    ];
    for (p, round, ns, bytes) in refused {
        let r = store.publish_transform_run(&p, round, ns, &bytes);
        assert!(matches!(r, Err(StoreError::Invalid(_))), "{p} {round} {ns}: {r:?}");
    }
    let runs = store.root().join("runs");
    let mut all: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&runs).unwrap() {
        for f in fs::read_dir(entry.unwrap().path()).unwrap() {
            all.push(f.unwrap().path());
        }
    }
    assert_eq!(all, vec![want]);
}

// ---------------------------------------------------------------------------
// The journal

/// `spec/exec.md` §2, `spec/edits.md` §7.3: a store that holds
/// `write.journal` is refused by an exclusive open (`UnfinishedWrite`, the
/// lock released), opened by a shared one, and by `open_for_recovery`,
/// which holds the lock.
#[test]
fn s7_3_journal_refuses_exclusive_open() {
    let dir = scratch("journal");
    let root = dir.join("store");
    let journal = exclusive(&dir).journal_path();
    assert_eq!(journal, root.join("write.journal"));
    fs::write(&journal, b"journal").unwrap();
    match Store::open(&root, Access::Exclusive) {
        Err(StoreError::UnfinishedWrite(p)) => assert_eq!(p, journal),
        other => panic!("{other:?}"),
    }
    assert!(!root.join("lock").exists(), "the refused open released the lock");
    let shared = Store::open(&root, Access::Shared).unwrap();
    assert_eq!(shared.journal_path(), journal);
    let recovery = Store::open_for_recovery(&root).unwrap();
    assert!(root.join("lock").exists());
    assert!(matches!(Store::open_for_recovery(&root), Err(StoreError::Busy { .. })));
    drop(recovery);
    drop(shared);
    assert!(journal.exists(), "no opener removes the journal");
    fs::remove_file(&journal).unwrap();
    drop(Store::open(&root, Access::Exclusive).unwrap());
    // Without a journal, recovery opens as an exclusive open does.
    drop(Store::open_for_recovery(&root).unwrap());
}
