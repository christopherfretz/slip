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
use slip_model::formats::undump;
use slip_model::hash::content_digest;
use slip_model::json;
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

/// A valid file of `kind` (`TRFM`, `EDTS`, `OVLY` or `RSLT`), one for each
/// `n`, from its JSON rendering (`spec/model.md` §10): the store validates
/// an object in full (`spec/exec.md` §2.2).
fn valid(kind: [u8; 4], n: u8) -> Vec<u8> {
    let h = |b: u8| format!("\"{}\"", Digest([b; 32]));
    let (k, section) = match &kind {
        b"TRFM" => (
            "TRFM",
            format!(
                "\"transform\": {{\"kind\": \"rename\", \"ownership\": {{\"owned\": [\"/s\"], \
                 \"readonly\": []}}, \"indivisible\": true, \"rename\": {{\"target\": [\"f\"], \
                 \"kind\": \"function\", \"new_name\": \"g{n}\", \"dependent\": \"offer\"}}, \
                 \"script\": null}}"
            ),
        ),
        b"EDTS" => (
            "EDTS",
            format!(
                "\"set\": {{\"scheme\": 1, \"plan\": {}, \"round\": {n}, \"transform\": {}, \
                 \"basis\": {}, \"state\": \"refused\", \"files\": [], \"subjects\": [], \
                 \"rules\": [], \"edits\": [], \"groups\": [], \"changed\": [], \"kept\": [], \
                 \"unresolved\": [], \"refusals\": [{{\"cause\": \"no_observer\", \"file\": null, \
                 \"lo\": 0, \"hi\": 0, \"reason\": \"none\", \"subject\": null, \"display\": null, \"macro\": null, \
                 \"other\": null, \"observers\": 0, \"first\": []}}], \"reports\": [], \
                 \"failures\": [], \"unparsed\": [], \"counters\": [], \"instances\": []}}",
                h(1),
                h(2),
                h(3)
            ),
        ),
        b"OVLY" => (
            "OVLY",
            format!(
                "\"overlay\": {{\"edits\": {}, \"files\": [{{\"path\": \"/s/a\", \
                 \"pre\": {{\"size\": 1, \"content\": {}}}, \
                 \"post\": {{\"size\": {n}, \"content\": {}}}}}], \"spellings\": []}}",
                h(4),
                h(5),
                h(6)
            ),
        ),
        b"RSLT" => (
            "RSLT",
            format!(
                "\"result\": {{\"scheme\": 1, \"plan\": {}, \"round\": {n}, \"transform\": {}, \
                 \"edits\": {}, \"declared\": null, \"overlay\": null, \"state\": \"refused\", \
                 \"partial\": false, \"allow_unparsed\": false, \"verdict\": null, \
                 \"tally\": {{\"selected\": 1, \"unparsed\": 1, \"failed_runs\": 0, \
                 \"error_reports\": 0, \"verified\": 0, \"unchanged\": 0, \"mismatch\": 0, \
                 \"not_parsed\": 0, \"unverifiable\": 0, \"never_parsed\": 0, \"failed\": 0}}, \
                 \"agreement\": [], \"unused\": [], \
                 \"spellings\": {{\"obligations\": 0, \"passed\": 0, \"failed\": []}}, \
                 \"residue\": [], \"exempt\": []}}",
                h(1),
                h(2),
                h(5)
            ),
        ),
        _ => panic!("no object of this kind"),
    };
    // RSLT is at version 2 (WP-111), TRFM at 2 (WP-139), EDTS at 3 (WP-160), OVLY at 1.
    let version = match &kind {
        b"OVLY" => 1,
        b"EDTS" => 3,
        _ => 2,
    };
    let text =
        format!("{{\"kind\": \"{k}\", \"version\": {version}, \"sections\": {{{section}}}}}");
    undump(&json::parse(text.as_bytes()).unwrap()).unwrap()
}

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
/// Publishing it again leaves the file untouched when it already holds that
/// content.
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
            let bytes = valid(kind, i);
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
        let again = store.publish_object(object, &valid(kind, 0)).unwrap();
        assert_eq!(again, ids[0]);
        assert_eq!(identity(&path), before);
    }
    // Each kind is found only in its own directory.
    let trfm = store.publish_object(Object::Transform, &valid(*b"TRFM", 99)).unwrap();
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
    // A version newer than any reader's (`EDTS` is at 2 since WP-139).
    let mut v2 = Writer::new(*b"EDTS", 3);
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

/// `spec/exec.md` §2.2: an object is validated in full. A container of the
/// right kind and frame whose section is not a valid file of the kind (an
/// edit set, an overlay, a result, a request), and a valid file but for
/// one invariant (an overlay with no file), are `Invalid`; nothing is
/// written.
#[test]
fn s2_2_object_validated_in_full() {
    let dir = scratch("full");
    let store = exclusive(&dir);
    for object in [Object::EditSet, Object::Overlay, Object::Result, Object::Transform] {
        match store.publish_object(object, &container(object.kind(), b"x")) {
            Err(StoreError::Invalid(m)) => {
                let kind = String::from_utf8_lossy(&object.kind()).into_owned();
                assert!(m.contains(&format!("not a valid {kind} file")), "{m}");
            }
            other => panic!("{object:?}: {other:?}"),
        }
        assert!(!store.root().join(object.dir()).exists(), "{object:?}");
    }
    // An overlay's section, well formed, with no file (`spec/edits.md` §7.1).
    let mut e = Encoder::new();
    e.digest(&d(4)).count(0).count(0);
    match store.publish_object(Object::Overlay, &container(*b"OVLY", e.as_bytes())) {
        Err(StoreError::Invalid(m)) => assert!(m.contains("an overlay has no file"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(!store.root().join("overlays").exists());
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

/// `spec/exec.md` §2, `spec/edits.md` §8: a transform's round is one more
/// than the greatest round the plan has taken (a shard directory of either
/// phase, a transform run record), and at least 1.
#[test]
fn s2_next_round() {
    let dir = scratch("rounds");
    let store = exclusive(&dir);
    let shards = |plan: Digest| store.root().join("shards").join(plan.to_string());
    // A fresh plan: 1, never 0 (round 0 is `slip parse`'s).
    let plan = d(0x50);
    assert_eq!(store.next_round(&plan).unwrap(), 1);
    // Only `map.0`: 1.
    fs::create_dir_all(shards(plan).join("map.0")).unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 1);
    // `map.0` and `verify.3`: 4.
    fs::create_dir_all(shards(plan).join("verify.3")).unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 4);
    // Names that are not rounds, and a plain file named as a shard
    // directory, take nothing.
    for name in ["map.07", "verify.x", "other.9", "map9", "verify.+8", "map.-1"] {
        fs::create_dir_all(shards(plan).join(name)).unwrap();
    }
    fs::write(shards(plan).join("map.9"), b"not a directory").unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 4);
    // A phase's run record takes no round.
    store.publish_run(&plan, Phase::Map, 7, 11, &phase_run(plan, Phase::Map, 7, 11)).unwrap();
    assert_eq!(store.next_round(&plan).unwrap(), 4);
    // Another plan's rounds are not this plan's.
    assert_eq!(store.next_round(&d(0x51)).unwrap(), 1);

    // Only a transform run record of round 2: 3.
    let other = d(0x52);
    store.publish_transform_run(&other, 2, 10, &transform_run(other, 2, 10)).unwrap();
    assert_eq!(store.next_round(&other).unwrap(), 3);

    // A gap (0, 1, 5): 6, not 2.
    let gap = d(0x53);
    fs::create_dir_all(shards(gap).join("map.0")).unwrap();
    fs::create_dir_all(shards(gap).join("verify.1")).unwrap();
    store.publish_transform_run(&gap, 5, 10, &transform_run(gap, 5, 10)).unwrap();
    assert_eq!(store.next_round(&gap).unwrap(), 6);
}

/// `spec/exec.md` §2: a transform run record's name has canonical decimal
/// `round` and `ns`; other names are not records.
#[test]
fn s2_transform_run_names_are_canonical() {
    let dir = scratch("runt-names");
    let store = exclusive(&dir);
    let plan = d(0x54);
    let runs = store.root().join("runs").join(plan.to_string());
    fs::create_dir_all(&runs).unwrap();
    let bytes = transform_run(plan, 1, 5);
    for name in
        ["transform.01.5.run", "transform.1.+5.run", "transform.1.05.run", "transform.1.run"]
    {
        fs::write(runs.join(name), &bytes).unwrap();
    }
    assert!(!store.is_transform_round(&plan, 1).unwrap());
    assert_eq!(store.next_round(&plan).unwrap(), 1);
    fs::write(runs.join("transform.1.5.run"), &bytes).unwrap();
    assert!(store.is_transform_round(&plan, 1).unwrap());
    assert_eq!(store.next_round(&plan).unwrap(), 2);
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

/// The payload of `transform_run(plan, round, started)` up to and
/// including `started_unix_ns`, then `rest`.
fn transform_run_payload(plan: Digest, round: u64, started: u64, rest: &[u8]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.str("s").str("k").digest(&plan).uv(round).digest(&d(0x77));
    e.u8(1).digest(&d(0x78)).u8(0).u8(1).digest(&d(0x79)).u8(0);
    e.uv(started);
    let mut out = e.into_bytes();
    out.extend_from_slice(rest);
    out
}

/// Publishes `bytes` as the record of `plan`, round 4, started 123: it
/// must be `Invalid` and leave no file.
fn refused_transform_run(name: &str, plan: Digest, bytes: &[u8]) {
    let dir = scratch(name);
    let store = exclusive(&dir);
    let r = store.publish_transform_run(&plan, 4, 123, bytes);
    assert!(matches!(r, Err(StoreError::Invalid(_))), "{r:?}");
    assert!(!store.root().join("runs").join(plan.to_string()).join("transform.4.123.run").exists());
}

/// `spec/exec.md` §2.2: the whole record is checked, not its three leading
/// names: one that stops after `started_unix_ns` is refused.
#[test]
fn s10_6_transform_run_truncated_after_started() {
    let plan = d(0x82);
    // The three names read; wall_ns and steps missing.
    let bytes = container(*b"RUNT", &transform_run_payload(plan, 4, 123, &[]));
    refused_transform_run("runt-short", plan, &bytes);
    // wall_ns present, steps missing.
    let bytes = container(*b"RUNT", &transform_run_payload(plan, 4, 123, &[5]));
    refused_transform_run("runt-short2", plan, &bytes);
    // Complete: published.
    let bytes = container(*b"RUNT", &transform_run_payload(plan, 4, 123, &[5, 0]));
    assert_eq!(bytes, transform_run(plan, 4, 123));
}

/// `spec/exec.md` §2.2: a record with a byte after its last field
/// (`steps`) is refused.
#[test]
fn s10_6_transform_run_trailing_bytes() {
    let plan = d(0x83);
    let bytes = container(*b"RUNT", &transform_run_payload(plan, 4, 123, &[5, 0, 0]));
    refused_transform_run("runt-trailing", plan, &bytes);
}

/// `spec/exec.md` §2.2, `spec/model.md` §5 rule 5: a record with an
/// unknown critical section (id 2) is refused; an unknown ignorable one
/// (id 64) is published.
#[test]
fn s10_6_transform_run_unknown_critical_section() {
    let plan = d(0x84);
    let payload = transform_run_payload(plan, 4, 123, &[5, 0]);
    let with = |id: u64| {
        let mut w = Writer::new(*b"RUNT", 1);
        w.section(1, &payload);
        w.section(id, b"x");
        w.finish()
    };
    refused_transform_run("runt-critical", plan, &with(2));
    let dir = scratch("runt-ignorable");
    let store = exclusive(&dir);
    store.publish_transform_run(&plan, 4, 123, &with(64)).unwrap();
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

/// `spec/exec.md` §2.2: an object or a blob is left untouched only when
/// the file holds that content. A blob changed on disk, an object replaced
/// by junk and a dangling symbolic link at the name are replaced by the
/// publish; a directory at the name is an `Io` error.
#[test]
fn s2_2_publish_repairs_what_is_not_the_content() {
    let dir = scratch("repair");
    let store = exclusive(&dir);
    // A blob changed on disk.
    let id = store.publish_blob(b"pre-image").unwrap();
    fs::write(store.blob_path(&id), b"pre-imagE").unwrap();
    assert!(store.read_blob(&id).is_err());
    assert_eq!(store.publish_blob(b"pre-image").unwrap(), id);
    assert_eq!(store.read_blob(&id).unwrap().as_deref(), Some(&b"pre-image"[..]));
    // An object replaced by junk.
    let bytes = valid(*b"OVLY", 7);
    let ov = store.publish_object(Object::Overlay, &bytes).unwrap();
    let path = store.object_path(Object::Overlay, &ov);
    fs::write(&path, b"junk").unwrap();
    assert_eq!(store.publish_object(Object::Overlay, &bytes).unwrap(), ov);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    // A dangling symbolic link at a blob's name, and one to a file that
    // holds the right bytes: both replaced by a regular file.
    for target in [dir.join("nowhere"), dir.join("right")] {
        fs::write(dir.join("right"), b"linked").unwrap();
        let id = content_digest(b"linked");
        let at = store.blob_path(&id);
        let _ = fs::remove_file(&at);
        std::os::unix::fs::symlink(&target, &at).unwrap();
        assert_eq!(store.publish_blob(b"linked").unwrap(), id);
        assert!(fs::symlink_metadata(&at).unwrap().is_file(), "{}", target.display());
        assert_eq!(store.read_blob(&id).unwrap().as_deref(), Some(&b"linked"[..]));
    }
    // A directory at the name.
    let id = content_digest(b"dir");
    fs::create_dir_all(store.blob_path(&id).join("x")).unwrap();
    assert!(matches!(store.publish_blob(b"dir"), Err(StoreError::Io { .. })));
    assert_eq!(fs::read_dir(store.root().join("tmp")).unwrap().count(), 0);
}

/// `spec/exec.md` §2: `write.journal` refuses an exclusive open whatever
/// kind of entry it is: a symbolic link (dangling or not) or a directory.
#[test]
fn s7_3_journal_of_any_kind_refuses() {
    let dir = scratch("journal-kinds");
    let root = dir.join("store");
    let journal = exclusive(&dir).journal_path();
    fs::write(dir.join("target"), b"j").unwrap();
    for what in ["dangling link", "link", "directory"] {
        match what {
            "dangling link" => std::os::unix::fs::symlink(dir.join("none"), &journal).unwrap(),
            "link" => std::os::unix::fs::symlink(dir.join("target"), &journal).unwrap(),
            _ => fs::create_dir(&journal).unwrap(),
        }
        match Store::open(&root, Access::Exclusive) {
            Err(StoreError::UnfinishedWrite(p)) => assert_eq!(p, journal, "{what}"),
            other => panic!("{what}: {other:?}"),
        }
        assert!(!root.join("lock").exists(), "{what}");
        drop(Store::open_for_recovery(&root).unwrap());
        if fs::symlink_metadata(&journal).unwrap().is_dir() {
            fs::remove_dir(&journal).unwrap();
        } else {
            fs::remove_file(&journal).unwrap();
        }
    }
}

/// `spec/exec.md` §2 "Cleaning": an exclusive open refused for the journal
/// leaves a dead process's stale `tmp/` entries in place (it refuses before
/// cleaning); `open_for_recovery` cleans them.
#[test]
fn s7_3_refused_open_leaves_stale_entries() {
    let dir = scratch("journal-stale");
    let root = dir.join("store");
    let journal = exclusive(&dir).journal_path();
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let dead = child.id();
    child.wait().unwrap();
    let stale = root.join("tmp").join(format!("{dead}.0.tmp"));
    let stale_dir = root.join("tmp").join(format!("{dead}.d"));
    fs::write(&stale, b"stale").unwrap();
    fs::create_dir_all(stale_dir.join("x")).unwrap();
    fs::write(&journal, b"journal").unwrap();
    assert!(matches!(Store::open(&root, Access::Exclusive), Err(StoreError::UnfinishedWrite(_))));
    assert!(stale.exists() && stale_dir.exists(), "the refused open cleaned");
    drop(Store::open_for_recovery(&root).unwrap());
    assert!(!stale.exists() && !stale_dir.exists(), "recovery did not clean");
}

/// `spec/exec.md` §2.2: the text of `UnfinishedWrite`.
#[test]
fn s2_2_unfinished_write_display() {
    let e = StoreError::UnfinishedWrite(PathBuf::from("/s/write.journal"));
    assert_eq!(
        e.to_string(),
        "the store has an unfinished write of the working tree (/s/write.journal); \
         run `slip apply --recover`"
    );
}
