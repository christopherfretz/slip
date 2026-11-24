//! `spec/edits.md` §7.3: `write` and `recover`. A tree in memory that
//! fails, or loses power, at the n-th operation, for every n, over a
//! three-file overlay; the same sweep over `recover`; the refusals; the
//! journal's record; and the disk implementation over a directory under
//! the target directory.

#![allow(
    clippy::many_single_char_names,
    clippy::type_complexity,
    reason = "short names for a case, a world and counters; one table of files"
)]

mod common;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;

use slip_apply::journal::{Journal, JournalEntry, staged_name};
use slip_apply::{Blobs, JournalStore, Recovered, Refused, Stat, Tree, WriteError, recover, write};
use slip_model::Digest;
use slip_model::formats;
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Overlay, OverlayFile};
use slip_model::transform::Ownership;

// ---------------------------------------------------------------------------
// A world in memory: the tree, the blobs and the journal, with one count
// of operations and one fault.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The operation fails and does nothing; the caller goes on.
    Fail,
    /// The operation takes effect, then reports a failure.
    FailAfter,
    /// Power is lost before the operation: it and every later one fail.
    Lose,
    /// Power is lost after the operation took effect.
    LoseAfter,
}

const MODES: [Mode; 4] = [Mode::Fail, Mode::FailAfter, Mode::Lose, Mode::LoseAfter];

#[derive(Clone, Debug, Default)]
struct Node {
    bytes: Vec<u8>,
    mode: u32,
    links: u64,
    regular: bool,
    uid: u32,
    gid: u32,
    dev: u64,
    ino: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

type Hook = Rc<dyn Fn(&mut State)>;

#[derive(Clone, Default)]
struct State {
    files: BTreeMap<Vec<u8>, Node>,
    readonly_dirs: BTreeSet<Vec<u8>>,
    blobs: BTreeMap<Digest, Vec<u8>>,
    journal: Option<Vec<u8>>,
    ops: u64,
    fault: Option<(u64, Mode)>,
    dead: bool,
    /// Every operation, with the path it names.
    log: Vec<(&'static str, Vec<u8>)>,
    /// Run just before the operation of this index.
    hook: Option<(u64, Hook)>,
    /// A second one, for a test that needs two.
    later: Option<(u64, Hook)>,
    /// `put` names its bytes by a wrong digest.
    lying_put: bool,
    /// The owner and group a create gives.
    creator: (u32, u32),
    /// The inode the next create gives.
    next_ino: u64,
}

#[derive(Clone)]
struct World(Rc<RefCell<State>>);

fn parent(p: &[u8]) -> Vec<u8> {
    let i = p.iter().rposition(|&b| b == b'/').unwrap();
    if i == 0 { b"/".to_vec() } else { p[..i].to_vec() }
}

fn err(kind: io::ErrorKind, what: &str) -> io::Error {
    io::Error::new(kind, what.to_string())
}

impl World {
    fn op<T>(
        &self,
        name: &'static str,
        path: &[u8],
        f: impl FnOnce(&mut State) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut s = self.0.borrow_mut();
        if s.dead {
            return Err(err(io::ErrorKind::Other, "no power"));
        }
        let n = s.ops;
        s.ops += 1;
        s.log.push((name, path.to_vec()));
        if let Some((at, hook)) = s.hook.clone()
            && at == n
        {
            hook(&mut s);
        }
        if let Some((at, hook)) = s.later.clone()
            && at == n
        {
            hook(&mut s);
        }
        match s.fault {
            Some((at, mode)) if at == n => {
                if matches!(mode, Mode::FailAfter | Mode::LoseAfter) {
                    let _ = f(&mut s);
                }
                if matches!(mode, Mode::Lose | Mode::LoseAfter) {
                    s.dead = true;
                }
                Err(err(io::ErrorKind::Other, "injected"))
            }
            _ => f(&mut s),
        }
    }

    fn healthy(&self) -> World {
        let mut s = self.0.borrow().clone();
        s.dead = false;
        s.fault = None;
        s.hook = None;
        s.later = None;
        s.ops = 0;
        s.log.clear();
        World(Rc::new(RefCell::new(s)))
    }

    fn with_fault(&self, at: u64, mode: Mode) -> World {
        let w = self.healthy();
        w.0.borrow_mut().fault = Some((at, mode));
        w
    }

    fn bytes(&self, p: &[u8]) -> Option<Vec<u8>> {
        self.0.borrow().files.get(p).map(|n| n.bytes.clone())
    }
}

impl Tree for World {
    fn stat(&self, p: &[u8]) -> io::Result<Option<Stat>> {
        self.op("stat", p, |s| {
            Ok(s.files.get(p).map(|n| Stat {
                regular: n.regular,
                links: n.links,
                mode: n.mode,
                dir_writable: !s.readonly_dirs.contains(&parent(p)),
                uid: n.uid,
                gid: n.gid,
                dev: n.dev,
                ino: n.ino,
                size: n.bytes.len() as u64,
                mtime: n.mtime,
                ctime: n.ctime,
            }))
        })
    }
    fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
        self.op("read", p, |s| {
            s.files
                .get(p)
                .map(|n| n.bytes.clone())
                .ok_or_else(|| err(io::ErrorKind::NotFound, "none"))
        })
    }
    fn create(&self, p: &[u8], mode: u32, bytes: &[u8]) -> io::Result<()> {
        self.op("create", p, |s| {
            if s.files.contains_key(p) {
                return Err(err(io::ErrorKind::AlreadyExists, "exists"));
            }
            if s.readonly_dirs.contains(&parent(p)) {
                return Err(err(io::ErrorKind::PermissionDenied, "read-only"));
            }
            let (uid, gid) = s.creator;
            s.next_ino += 1;
            let ino = 1000 + s.next_ino;
            s.files.insert(
                p.to_vec(),
                Node {
                    bytes: bytes.to_vec(),
                    mode,
                    links: 1,
                    regular: true,
                    uid,
                    gid,
                    ino,
                    ..Node::default()
                },
            );
            Ok(())
        })
    }
    fn rename(&self, from: &[u8], to: &[u8]) -> io::Result<()> {
        self.op("rename", to, |s| {
            let n = s.files.remove(from).ok_or_else(|| err(io::ErrorKind::NotFound, "none"))?;
            s.files.insert(to.to_vec(), n);
            Ok(())
        })
    }
    fn remove(&self, p: &[u8]) -> io::Result<()> {
        self.op("remove", p, |s| {
            s.files.remove(p).map(|_| ()).ok_or_else(|| err(io::ErrorKind::NotFound, "none"))
        })
    }
    fn sync_dir(&self, d: &[u8]) -> io::Result<()> {
        self.op("sync_dir", d, |_| Ok(()))
    }
    /// Not an operation of the sweep: a listing changes nothing, and
    /// this world spells every name one way. The directories are those
    /// on the paths of its files.
    fn names(&self, d: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        let s = self.0.borrow();
        let prefix: Vec<u8> = if d == b"/" { b"/".to_vec() } else { [d, b"/"].concat() };
        let mut names = BTreeSet::new();
        for p in s.files.keys() {
            if let Some(rest) = p.strip_prefix(prefix.as_slice()) {
                let end = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
                names.insert(rest[..end].to_vec());
            }
        }
        Ok(if names.is_empty() { None } else { Some(names.into_iter().collect()) })
    }
}

impl Blobs for World {
    fn get(&self, id: Digest) -> io::Result<Vec<u8>> {
        self.op("get", b"", |s| {
            s.blobs.get(&id).cloned().ok_or_else(|| err(io::ErrorKind::NotFound, "no blob"))
        })
    }
    fn put(&self, bytes: &[u8]) -> io::Result<Digest> {
        self.op("put", b"", |s| {
            let d = content_digest(bytes);
            s.blobs.insert(d, bytes.to_vec());
            Ok(if s.lying_put { Digest([0; 32]) } else { d })
        })
    }
}

impl JournalStore for World {
    fn publish(&self, bytes: &[u8]) -> io::Result<()> {
        self.op("publish", b"", |s| {
            if s.journal.is_some() {
                return Err(err(io::ErrorKind::AlreadyExists, "a journal exists"));
            }
            s.journal = Some(bytes.to_vec());
            Ok(())
        })
    }
    fn read(&self) -> io::Result<Option<Vec<u8>>> {
        self.op("journal", b"", |s| Ok(s.journal.clone()))
    }
    fn remove(&self) -> io::Result<()> {
        self.op("unjournal", b"", |s| {
            s.journal = None;
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// The scenario: three files to replace (two directories), one entry with
// post equal to pre, and a file the overlay does not name.

const SAME: &[u8] = b"/w/same.h";
const OTHER: &[u8] = b"/w/other.txt";

struct Case {
    overlay: Overlay,
    id: Digest,
    pre: Vec<(Vec<u8>, Vec<u8>)>,
    post: Vec<(Vec<u8>, Vec<u8>)>,
    world: World,
}

fn content(b: &[u8]) -> Content {
    Content { size: b.len() as u64, content: content_digest(b) }
}

fn case() -> Case {
    let files: Vec<(&[u8], &[u8], &[u8], u32)> = vec![
        (b"/w/a.cc", b"int a;\r\n", b"int alpha;\r\n", 0o644),
        (b"/w/b.h", b"#pragma once\nint b", b"#pragma once\nint beta", 0o600),
        (b"/w/sub/c.sh", b"#!/bin/sh\nx\n", b"#!/bin/sh\ny\n", 0o755),
        (SAME, b"same\n", b"same\n", 0o644),
    ];
    let mut sorted = files.clone();
    sorted.sort();
    let overlay = Overlay {
        edits: Digest([7; 32]),
        files: sorted
            .iter()
            .map(|(p, a, b, _)| OverlayFile { path: p.to_vec(), pre: content(a), post: content(b) })
            .collect(),
        spellings: vec![],
    };
    let id = formats::content_id(&overlay.to_bytes()).unwrap();
    let mut s = State::default();
    for (p, a, b, mode) in &files {
        s.files.insert(
            p.to_vec(),
            Node { bytes: a.to_vec(), mode: *mode, links: 1, regular: true, ..Node::default() },
        );
        s.blobs.insert(content_digest(b), b.to_vec());
    }
    s.files.insert(
        OTHER.to_vec(),
        Node { bytes: b"other".to_vec(), mode: 0o644, links: 1, regular: true, ..Node::default() },
    );
    let changed = files.iter().filter(|f| f.1 != f.2);
    Case {
        overlay,
        id,
        pre: changed.clone().map(|f| (f.0.to_vec(), f.1.to_vec())).collect(),
        post: changed.map(|f| (f.0.to_vec(), f.2.to_vec())).collect(),
        world: World(Rc::new(RefCell::new(s))),
    }
}

fn owned(entries: &[&str]) -> Ownership {
    Ownership { owned: entries.iter().map(|e| e.as_bytes().to_vec()).collect(), readonly: vec![] }
}

/// The ownership of the in-memory tree's writes.
fn own_w() -> Ownership {
    owned(&["/w"])
}

fn run_write(c: &Case, w: &World) -> Result<u64, WriteError> {
    write(&c.overlay, c.id, &owned(&["/w"]), w, w, w)
}

/// Whether the tree is wholly the post-image. Panics on a file that is
/// neither image, and on a mix of images without a journal. Also checks the files
/// it does not name, the modes, and that no name exists but the tree's
/// own and, while a journal is there, the staged names it holds.
fn image(c: &Case, w: &World) -> bool {
    let s = w.0.borrow();
    let mut names: BTreeSet<Vec<u8>> = c.pre.iter().map(|f| f.0.clone()).collect();
    names.insert(SAME.to_vec());
    names.insert(OTHER.to_vec());
    let staged: BTreeSet<Vec<u8>> = c.pre.iter().map(|f| staged_name(&f.0, c.id)).collect();
    for p in s.files.keys() {
        assert!(
            names.contains(p) || (s.journal.is_some() && staged.contains(p)),
            "stray {:?}",
            String::from_utf8_lossy(p)
        );
    }
    assert_eq!(s.files[SAME].bytes, b"same\n");
    assert_eq!(s.files[OTHER].bytes, b"other");
    let modes: BTreeMap<&[u8], u32> =
        [(&b"/w/a.cc"[..], 0o644), (b"/w/b.h", 0o600), (b"/w/sub/c.sh", 0o755)].into();
    let mut kinds = BTreeSet::new();
    for ((p, pre), (_, post)) in c.pre.iter().zip(&c.post) {
        let n = &s.files[p];
        assert_eq!(n.mode, modes[p.as_slice()], "mode of {:?}", String::from_utf8_lossy(p));
        kinds.insert(if &n.bytes == pre {
            false
        } else if &n.bytes == post {
            true
        } else {
            panic!("{:?} is neither image: {:?}", String::from_utf8_lossy(p), n.bytes)
        });
    }
    // Files of both images only while a journal says how to finish.
    assert!(kinds.len() == 1 || s.journal.is_some(), "a mix of images and no journal");
    kinds.len() == 1 && kinds.contains(&true)
}

/// `recover` on a healthy world returns the tree to the pre-image and
/// removes the journal and staged files; a second run does nothing.
fn recover_fully(c: &Case, w: &World) {
    let r = recover(&own_w(), w, w, w).expect("recover");
    let r = r.expect("a journal");
    assert!(r.reported.is_empty(), "{r:?}");
    assert!(w.0.borrow().journal.is_none());
    assert!(!image(c, w));
    let before = w.0.borrow().ops;
    assert_eq!(recover(&own_w(), w, w, w), Ok(None));
    assert_eq!(w.0.borrow().ops, before + 1, "the second recover only reads the journal");
}

/// `SAME` (post equal to pre) is never named by an operation.
fn same_untouched(w: &World) {
    let staged_prefix = b"/w/.same.h".to_vec();
    for (op, p) in &w.0.borrow().log {
        assert!(
            p.as_slice() != SAME && !p.starts_with(&staged_prefix),
            "{op} on {:?}",
            String::from_utf8_lossy(p)
        );
    }
}

// §7.3: a write without failure replaces the three files, keeps their
// modes, leaves no journal and no staged file, and touches neither the
// entry with post equal to pre nor a file the overlay does not name.
#[test]
fn s7_3_write_replaces_every_file() {
    let c = case();
    let w = c.world.healthy();
    assert_eq!(run_write(&c, &w), Ok(3));
    assert!(image(&c, &w));
    assert!(w.0.borrow().journal.is_none());
    same_untouched(&w);
    // The pre-images are blobs of the store.
    for (_, pre) in &c.pre {
        assert_eq!(w.0.borrow().blobs.get(&content_digest(pre)), Some(pre));
    }
    assert_eq!(recover(&own_w(), &w, &w, &w), Ok(None));
}

/// The number of operations of a run without failures.
fn clean_ops(c: &Case) -> u64 {
    let w = c.world.healthy();
    assert_eq!(run_write(c, &w), Ok(3));
    w.0.borrow().ops
}

// §7.3, the crash sweep: a failure injected at every operation of `write`,
// in each of four modes, leaves every file wholly pre or wholly post; if a
// journal is left, `recover` returns the tree to the pre-image; if none,
// the tree is the pre-image or, only after the last operation took
// effect, the post-image; `journal_left` false means pre and no journal.
#[test]
fn s7_3_crash_sweep_over_write() {
    let c = case();
    let n = clean_ops(&c);
    let mut visited = 0;
    for mode in MODES {
        for at in 0..n {
            let w = c.world.with_fault(at, mode);
            let got = run_write(&c, &w);
            assert!(w.0.borrow().ops > at, "fault {at} was reached");
            visited += 1;
            same_untouched(&w);
            let post = image(&c, &w);
            let journal = w.0.borrow().journal.is_some();
            match &got {
                Ok(_) => panic!("write succeeded with fault {at} {mode:?}"),
                Err(WriteError::Io { journal_left: false, .. } | WriteError::Refused(_)) => {
                    assert!(!journal && !post, "fault {at} {mode:?}: {got:?}");
                }
                Err(WriteError::Io { journal_left: true, .. }) => {}
                Err(e) => panic!("{e:?}"),
            }
            if journal {
                recover_fully(&c, &w.healthy());
            } else if post {
                // Only the removal of the journal, the last operation, can
                // have finished the write and then failed.
                assert!(
                    at == n - 1 && matches!(mode, Mode::FailAfter | Mode::LoseAfter),
                    "fault {at} {mode:?}"
                );
            }
        }
    }
    assert_eq!(visited, 4 * n);
    eprintln!("write sweep: {n} operations, {visited} faults");
}

// §7.3: the same sweep over `recover`: from every state an interrupted
// write can leave a journal in, a recover interrupted at each of its
// operations, in each mode, then a recover that runs, ends in the
// pre-image with no journal and no staged file.
#[test]
fn s7_3_crash_sweep_over_recover() {
    let c = case();
    let n = clean_ops(&c);
    let mut visited = 0;
    let mut states = 0;
    for at in 0..n {
        let w = c.world.with_fault(at, Mode::LoseAfter);
        let _ = run_write(&c, &w);
        if w.0.borrow().journal.is_none() {
            continue;
        }
        states += 1;
        let start = w.healthy();
        let m = {
            let probe = start.healthy();
            recover(&own_w(), &probe, &probe, &probe).unwrap().unwrap();
            probe.0.borrow().ops
        };
        for mode in MODES {
            for k in 0..m {
                let r = start.with_fault(k, mode);
                let _ = recover(&own_w(), &r, &r, &r);
                assert!(r.0.borrow().ops > k);
                visited += 1;
                image(&c, &r);
                let again = r.healthy();
                match recover(&own_w(), &again, &again, &again) {
                    Ok(Some(rec)) => assert!(rec.reported.is_empty(), "{rec:?}"),
                    Ok(None) => {}
                    Err(e) => panic!("{e:?}"),
                }
                assert!(again.0.borrow().journal.is_none());
                assert!(!image(&c, &again));
                assert_eq!(recover(&own_w(), &again, &again, &again), Ok(None));
            }
        }
    }
    assert!(states > 20 && visited > 1000, "{states} states, {visited} faults");
    eprintln!("recover sweep: {states} states, {visited} faults");
}

// §7.3 step 1: each refusal, with nothing touched: no journal, no
// staged file, no blob, no mutating operation.
#[test]
fn s7_3_refusals_touch_nothing() {
    let check = |setup: &dyn Fn(&mut State), own: &[&str], want: Refused| {
        let c = case();
        setup(&mut c.world.0.borrow_mut());
        let w = c.world.healthy();
        let blobs = w.0.borrow().blobs.len();
        let got = write(&c.overlay, c.id, &owned(own), &w, &w, &w);
        assert_eq!(got, Err(WriteError::Refused(want)));
        let s = w.0.borrow();
        assert!(s.journal.is_none());
        assert_eq!(s.blobs.len(), blobs);
        assert!(
            s.log.iter().all(|(op, _)| matches!(*op, "stat" | "read" | "journal")),
            "{:?}",
            s.log
        );
    };
    let a = b"/w/a.cc".to_vec();
    let b = b"/w/b.h".to_vec();
    let cc = b"/w/sub/c.sh".to_vec();
    check(&|_| {}, &["/w/sub"], Refused::NotOwned(a.clone()));
    check(&|s| s.files.get_mut(&b).unwrap().links = 2, &["/w"], Refused::HardLinked(b.clone()));
    check(
        &|s| s.files.get_mut(&b).unwrap().regular = false,
        &["/w"],
        Refused::NotRegular(b.clone()),
    );
    check(
        &|s| {
            s.readonly_dirs.insert(b"/w/sub".to_vec());
        },
        &["/w"],
        Refused::NotWritable(cc.clone()),
    );
    check(
        &|s| s.files.get_mut(&a).unwrap().bytes.push(b'!'),
        &["/w"],
        Refused::StaleFile(a.clone()),
    );
    check(
        &|s| {
            s.files.remove(&cc);
        },
        &["/w"],
        Refused::StaleFile(cc.clone()),
    );
    let c0 = case();
    let staged = staged_name(&b, c0.id);
    check(
        &|s| {
            s.files.insert(
                staged.clone(),
                Node { bytes: vec![], mode: 0o644, links: 1, regular: true, ..Node::default() },
            );
        },
        &["/w"],
        Refused::StagedExists(staged.clone()),
    );
    // Any node at the staged name: a link, a directory, a device.
    check(
        &|s| {
            s.files.insert(
                staged.clone(),
                Node { mode: 0o777, links: 1, regular: false, ..Node::default() },
            );
        },
        &["/w"],
        Refused::StagedExists(staged.clone()),
    );
    // Its owner's write bit clear (a file not opened for edit).
    check(&|s| s.files.get_mut(&b).unwrap().mode = 0o444, &["/w"], Refused::ReadOnly(b.clone()));
    // A readonly entry carves a file out of an owned directory.
    let c = case();
    let w = c.world.healthy();
    let own = Ownership { owned: vec![b"/w".to_vec()], readonly: vec![b"/w/b.h".to_vec()] };
    assert_eq!(
        write(&c.overlay, c.id, &own, &w, &w, &w),
        Err(WriteError::Refused(Refused::NotOwned(b)))
    );
}

// §7.3 step 4: a file changed by someone else between the two checks: the
// staged files and the journal are removed, nothing is replaced, and the
// other writer's bytes survive.
#[test]
fn s7_3_file_changed_between_the_checks() {
    let c = case();
    let clean = c.world.healthy();
    run_write(&c, &clean).unwrap();
    // The operation after the last create of step 3.
    let last_create =
        clean.0.borrow().log.iter().rposition(|(op, _)| *op == "create").unwrap() as u64;
    let w = c.world.healthy();
    w.0.borrow_mut().hook = Some((
        last_create + 1,
        Rc::new(|s: &mut State| {
            s.files.get_mut(b"/w/b.h".as_slice()).unwrap().bytes = b"someone else's".to_vec();
        }),
    ));
    assert_eq!(run_write(&c, &w), Err(WriteError::Refused(Refused::StaleFile(b"/w/b.h".to_vec()))));
    let s = w.0.borrow();
    assert!(s.journal.is_none());
    assert_eq!(s.files[b"/w/b.h".as_slice()].bytes, b"someone else's");
    assert_eq!(s.files[b"/w/a.cc".as_slice()].bytes, c.pre[0].1);
    assert_eq!(s.files[b"/w/sub/c.sh".as_slice()].bytes, c.pre[2].1);
    assert!(s.files.keys().all(|p| !p.windows(6).any(|x| x == b".slip-")));
}

// §7.3: a staged file replaced by someone between step 3 and step 4 is
// found by the second check (a staged file must be what step 3 wrote).
#[test]
fn s7_3_staged_file_changed_before_the_commit() {
    let c = case();
    let clean = c.world.healthy();
    run_write(&c, &clean).unwrap();
    let last_create =
        clean.0.borrow().log.iter().rposition(|(op, _)| *op == "create").unwrap() as u64;
    let staged = staged_name(b"/w/a.cc", c.id);
    let w = c.world.healthy();
    let st = staged.clone();
    w.0.borrow_mut().hook = Some((
        last_create + 1,
        Rc::new(move |s: &mut State| {
            s.files.get_mut(&st).unwrap().bytes = b"tampered".to_vec();
        }),
    ));
    assert_eq!(run_write(&c, &w), Err(WriteError::Refused(Refused::StaleFile(staged))));
    assert!(!image(&c, &w));
    assert!(w.0.borrow().journal.is_none());
}

// §7.3, recovery: a file changed by someone else after the commit and
// before `recover` is reported and left; the others are restored; the
// journal stays, and a second recover reports the same.
#[test]
fn s7_3_file_changed_after_the_commit_is_reported_and_left() {
    let c = case();
    let clean = c.world.healthy();
    run_write(&c, &clean).unwrap();
    let last_rename =
        clean.0.borrow().log.iter().rposition(|(op, _)| *op == "rename").unwrap() as u64;
    let w = c.world.with_fault(last_rename, Mode::LoseAfter);
    let _ = run_write(&c, &w);
    let w = w.healthy();
    w.0.borrow_mut().files.get_mut(b"/w/b.h".as_slice()).unwrap().bytes =
        b"edited by hand".to_vec();
    let want = Recovered {
        restored: vec![b"/w/a.cc".to_vec(), b"/w/sub/c.sh".to_vec()],
        left: vec![],
        reported: vec![b"/w/b.h".to_vec()],
    };
    assert_eq!(recover(&own_w(), &w, &w, &w), Ok(Some(want)));
    assert!(w.0.borrow().journal.is_some());
    assert_eq!(w.bytes(b"/w/b.h").unwrap(), b"edited by hand");
    assert_eq!(w.bytes(b"/w/a.cc").unwrap(), c.pre[0].1);
    let again = recover(&own_w(), &w, &w, &w).unwrap().unwrap();
    assert_eq!(again.reported, vec![b"/w/b.h".to_vec()]);
    assert_eq!(again.left, vec![b"/w/a.cc".to_vec(), b"/w/sub/c.sh".to_vec()]);
    // A file that is gone, or became a link, is reported too.
    w.0.borrow_mut().files.remove(b"/w/b.h".as_slice());
    assert_eq!(recover(&own_w(), &w, &w, &w).unwrap().unwrap().reported, vec![b"/w/b.h".to_vec()]);
}

/// The index of the `nth` operation named `op` in a run without failures.
fn op_index(c: &Case, op: &str, nth: usize) -> u64 {
    let clean = c.world.healthy();
    run_write(c, &clean).unwrap();
    let log = &clean.0.borrow().log;
    log.iter().enumerate().filter(|(_, (o, _))| *o == op).nth(nth).unwrap().0 as u64
}

/// A write with `hook` run before operation `at`.
fn hooked(c: &Case, at: u64, hook: impl Fn(&mut State) + 'static) -> World {
    let w = c.world.healthy();
    w.0.borrow_mut().hook = Some((at, Rc::new(hook)));
    w
}

// §7.3 step 4: a staged file of the right bytes that has become a hard
// link, or is no longer a regular file, is not what step 3 wrote.
#[test]
fn s7_3_staged_file_linked_or_replaced() {
    let c = case();
    let after = op_index(&c, "create", 2) + 1;
    let staged = staged_name(b"/w/b.h", c.id);
    for links in [true, false] {
        let st = staged.clone();
        let w = hooked(&c, after, move |s: &mut State| {
            let n = s.files.get_mut(&st).unwrap();
            if links { n.links = 2 } else { n.regular = false }
        });
        assert_eq!(run_write(&c, &w), Err(WriteError::Refused(Refused::StaleFile(staged.clone()))));
        assert!(w.0.borrow().journal.is_none());
    }
}

// §7.3 step 2: the bytes are read again before they are journaled; a
// change after step 1 is refused before the journal exists.
#[test]
fn s7_3_file_changed_before_the_journal() {
    let c = case();
    let at = op_index(&c, "put", 1) - 1; // the read of step 2 for /w/b.h
    let w = hooked(&c, at, |s: &mut State| {
        s.files.get_mut(b"/w/b.h".as_slice()).unwrap().bytes = b"changed".to_vec();
    });
    assert_eq!(run_write(&c, &w), Err(WriteError::Refused(Refused::StaleFile(b"/w/b.h".to_vec()))));
    let s = w.0.borrow();
    assert!(s.journal.is_none());
    assert!(s.log.iter().all(|(op, _)| !matches!(*op, "publish" | "create" | "rename")));
}

// §7.3 step 2: a blob store that names the pre-image by another digest
// stops the write before the journal.
#[test]
fn s7_3_blob_store_with_a_wrong_digest() {
    let c = case();
    let w = c.world.healthy();
    w.0.borrow_mut().lying_put = true;
    assert!(matches!(run_write(&c, &w), Err(WriteError::Io { journal_left: false, .. })));
    assert!(w.0.borrow().journal.is_none());
    assert!(!image(&c, &w));
}

// §7.3 step 3: a staged name that appears (someone else's file) between
// the check and the exclusive create is not removed by the undo.
#[test]
fn s7_3_staged_name_taken_by_someone_else() {
    let c = case();
    let at = op_index(&c, "create", 1);
    let staged = staged_name(b"/w/b.h", c.id);
    let st = staged.clone();
    let w = hooked(&c, at, move |s: &mut State| {
        s.files.insert(
            st.clone(),
            Node {
                bytes: b"theirs".to_vec(),
                mode: 0o644,
                links: 1,
                regular: true,
                ..Node::default()
            },
        );
    });
    assert!(matches!(run_write(&c, &w), Err(WriteError::Io { journal_left: false, .. })));
    assert_eq!(w.bytes(&staged).unwrap(), b"theirs");
    assert!(w.bytes(&staged_name(b"/w/a.cc", c.id)).is_none());
    assert!(w.0.borrow().journal.is_none());
}

// §7.3 step 5 and recovery: every directory is synced after the last
// rename and before the journal is removed (the in-memory tree cannot
// lose a rename, so the order is what is checked).
#[test]
fn s7_3_directories_synced_before_the_journal_is_removed() {
    let synced = |log: &[(&'static str, Vec<u8>)]| {
        let last_rename = log.iter().rposition(|(o, _)| *o == "rename").unwrap();
        let gone = log.iter().rposition(|(o, _)| *o == "unjournal").unwrap();
        let dirs: BTreeSet<Vec<u8>> = log[last_rename..gone]
            .iter()
            .filter(|(o, _)| *o == "sync_dir")
            .map(|(_, p)| p.clone())
            .collect();
        assert_eq!(dirs, BTreeSet::from([b"/w".to_vec(), b"/w/sub".to_vec()]), "{log:?}");
    };
    let c = case();
    let w = c.world.healthy();
    run_write(&c, &w).unwrap();
    synced(&w.0.borrow().log);
    // Recovery from a finished commit whose journal remained.
    let last = op_index(&c, "unjournal", 0);
    let w = c.world.with_fault(last, Mode::Lose);
    let _ = run_write(&c, &w);
    let r = w.healthy();
    recover(&own_w(), &r, &r, &r).unwrap().unwrap();
    synced(&r.0.borrow().log);
}

/// Whether a world holds no journal, no staged file and the pre-image.
fn untouched(c: &Case, w: &World) {
    assert!(w.0.borrow().journal.is_none());
    assert!(!image(c, w));
}

// §7.3 step 4: the file itself is checked again: a hard link, a node that
// is no longer a regular file, a directory no longer writable, an owner
// write bit cleared, after step 3, each refuse and undo.
#[test]
fn s7_3_step_4_rechecks_the_file_itself() {
    let c = case();
    let after = op_index(&c, "create", 2) + 1;
    let b = b"/w/b.h".to_vec();
    let cases: Vec<(Box<dyn Fn(&mut State)>, Refused)> = vec![
        (
            Box::new(|s| s.files.get_mut(b"/w/b.h".as_slice()).unwrap().links = 2),
            Refused::HardLinked(b.clone()),
        ),
        (
            Box::new(|s| s.files.get_mut(b"/w/b.h".as_slice()).unwrap().regular = false),
            Refused::NotRegular(b.clone()),
        ),
        (
            Box::new(|s| s.files.get_mut(b"/w/b.h".as_slice()).unwrap().mode = 0o400),
            Refused::ReadOnly(b.clone()),
        ),
        (
            Box::new(|s| {
                s.readonly_dirs.insert(b"/w".to_vec());
            }),
            Refused::NotWritable(b"/w/a.cc".to_vec()),
        ),
    ];
    for (change, want) in cases {
        let change = Rc::new(change);
        let w = hooked(&c, after, move |s: &mut State| change(s));
        assert_eq!(run_write(&c, &w), Err(WriteError::Refused(want)));
        // Refused at step 4 itself, not later by the re-check before a rename.
        assert!(w.0.borrow().log.iter().all(|(o, _)| *o != "rename"));
        let mut s = w.0.borrow_mut();
        s.readonly_dirs.clear();
        let n = s.files.get_mut(b"/w/b.h".as_slice()).unwrap();
        (n.links, n.regular, n.mode) = (1, true, 0o600);
        drop(s);
        untouched(&c, &w);
    }
}

// §7.3 step 4: a staged file whose owner or group is not the file's (a
// create by another user, or in a setgid directory) refuses and undoes.
#[test]
fn s7_3_staged_owner_or_group_differs() {
    for creator in [(0, 7), (7, 0)] {
        let c = case();
        let w = c.world.healthy();
        w.0.borrow_mut().creator = creator;
        assert_eq!(
            run_write(&c, &w),
            Err(WriteError::Refused(Refused::OwnerChanged(b"/w/a.cc".to_vec())))
        );
        untouched(&c, &w);
    }
}

// §7.3 step 5: the re-check just before each rename. Every field it
// compares, changed between step 4 and the rename of /w/b.h (after /w/a.cc
// was renamed), refuses it; the write rolls back to the pre-image and the
// refusal names the file, or its staged file.
#[test]
fn s7_3_each_rename_is_preceded_by_a_recheck() {
    let c = case();
    // Before the re-check of /w/b.h: just after the rename of /w/a.cc.
    let at = op_index(&c, "rename", 0) + 1;
    let b = b"/w/b.h".to_vec();
    let sb = staged_name(&b, c.id);
    let fields: [(&str, fn(&mut Node)); 9] = [
        ("dev", |n| n.dev += 1),
        ("ino", |n| n.ino += 1),
        ("size", |n| n.bytes.push(b'!')),
        ("mtime", |n| n.mtime.0 += 1),
        ("mtime ns", |n| n.mtime.1 += 1),
        ("ctime", |n| n.ctime.0 += 1),
        ("ctime ns", |n| n.ctime.1 += 1),
        ("regular", |n| n.regular = false),
        ("links", |n| n.links = 2),
    ];
    for (target, name) in [(&b, &b), (&sb, &sb)] {
        for (field, change) in fields {
            let t = target.clone();
            let w = hooked(&c, at, move |s: &mut State| change(s.files.get_mut(&t).unwrap()));
            let got = run_write(&c, &w);
            if target == &b && matches!(field, "size" | "regular") {
                // A save that changed the bytes, or a node that is no
                // longer a file: the rollback reports it, keeps it, and
                // leaves the journal.
                assert!(matches!(got, Err(WriteError::Io { journal_left: true, .. })), "{got:?}");
                let want = if field == "size" {
                    [&c.pre[1].1[..], b"!"].concat()
                } else {
                    c.pre[1].1.clone()
                };
                assert_eq!(w.bytes(&b).unwrap(), want);
                assert_eq!(w.bytes(b"/w/a.cc").unwrap(), c.pre[0].1);
                continue;
            }
            let want = if target == &b && field == "links" {
                Refused::HardLinked(b.clone())
            } else {
                Refused::StaleFile(name.clone())
            };
            assert_eq!(got, Err(WriteError::Refused(want)), "{field} of {}", text(target));
            let s = w.0.borrow();
            assert!(s.journal.is_none());
            assert_eq!(s.files[b"/w/a.cc".as_slice()].bytes, c.pre[0].1, "{field}");
            assert!(s.files.keys().all(|p| !p.windows(6).any(|x| x == b".slip-")));
            assert_eq!(s.log.iter().filter(|(o, _)| *o == "rename").count(), 2, "{field}");
        }
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// A world after an interrupted commit: every file post, the journal left.
fn after_commit(c: &Case) -> World {
    let w = c.world.with_fault(op_index(c, "unjournal", 0), Mode::Lose);
    let _ = run_write(c, &w);
    let w = w.healthy();
    assert!(w.0.borrow().journal.is_some() && image(c, &w));
    w
}

// §7.3 recovery: a file that is no longer a regular file is reported,
// whatever its bytes (never read through a link and called left).
#[test]
fn s7_3_recover_reports_a_file_no_longer_regular() {
    let c = case();
    // /w/a.cc renamed (post), /w/b.h still pre.
    let w = c.world.with_fault(op_index(&c, "rename", 0), Mode::LoseAfter);
    let _ = run_write(&c, &w);
    let w = w.healthy();
    w.0.borrow_mut().files.get_mut(b"/w/b.h".as_slice()).unwrap().regular = false;
    let got = recover(&own_w(), &w, &w, &w).unwrap().unwrap();
    assert_eq!(got.reported, vec![b"/w/b.h".to_vec()]);
    assert_eq!(got.restored, vec![b"/w/a.cc".to_vec()]);
    assert!(w.0.borrow().journal.is_some());
    assert!(w.0.borrow().log.iter().all(|(op, p)| *op != "read" || p != b"/w/b.h"));
}

// §7.3 recovery: the staged name is created exclusively: a file that
// appears there before recover's create is neither replaced nor removed,
// recover fails and the journal stays.
#[test]
fn s7_3_recover_creates_the_staged_name_exclusively() {
    let c = case();
    let start = after_commit(&c);
    let probe = start.healthy();
    recover(&own_w(), &probe, &probe, &probe).unwrap().unwrap();
    // Before the stat that follows recover's own clearing of the staged
    // name: the name appears after recover last removed it, so a create
    // that cleared the name again (not exclusive) would overwrite it.
    let first_create =
        probe.0.borrow().log.iter().position(|(o, _)| *o == "remove").unwrap() as u64 + 1;
    let w = start.healthy();
    let st = staged_name(b"/w/a.cc", c.id);
    let s2 = st.clone();
    w.0.borrow_mut().hook = Some((
        first_create,
        Rc::new(move |s: &mut State| {
            s.files.insert(
                s2.clone(),
                Node {
                    bytes: b"theirs".to_vec(),
                    mode: 0o644,
                    links: 1,
                    regular: true,
                    ..Node::default()
                },
            );
        }),
    ));
    assert!(matches!(
        recover(&own_w(), &w, &w, &w),
        Err(WriteError::Io { journal_left: true, .. })
    ));
    assert_eq!(w.bytes(&st).unwrap(), b"theirs");
    assert_eq!(w.bytes(b"/w/a.cc").unwrap(), c.post[0].1);
    assert!(w.0.borrow().journal.is_some());
}

/// Every file of `w` given times with a nanosecond part.
fn fine_times(w: &World) {
    for n in w.0.borrow_mut().files.values_mut() {
        (n.mtime, n.ctime) = ((1, 5), (1, 7));
    }
}

// §7.3 step 5: with times that have a nanosecond part, a new inode number
// alone (a volume that renumbers files after a rename) is not a change,
// before the commit's renames and before the restore's; with coarse times
// (the other tests: a zero nanosecond part) it is.
#[test]
fn s7_3_a_renumbered_inode_with_fine_times() {
    let c = case();
    let at = op_index(&c, "rename", 0) + 1;
    let w = hooked(&c, at, |s: &mut State| s.files.get_mut(b"/w/b.h".as_slice()).unwrap().ino += 1);
    fine_times(&w);
    assert_eq!(run_write(&c, &w), Ok(3));
    assert!(image(&c, &w));
    let start = after_commit(&c);
    fine_times(&start);
    let at = index_in(&start, "rename", 0, |w| {
        recover(&own_w(), w, w, w).unwrap().unwrap();
    }) - 1;
    let w = start.healthy();
    w.0.borrow_mut().hook = Some((
        at,
        Rc::new(|s: &mut State| s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().ino += 1),
    ));
    let got = recover(&own_w(), &w, &w, &w).unwrap().unwrap();
    assert!(got.reported.is_empty());
    assert!(got.restored.contains(&b"/w/a.cc".to_vec()));
    assert!(w.0.borrow().journal.is_none());
    // The same with a zero nanosecond part in both times, or in one of
    // them: refused.
    for (m, ct) in [(0, 0), (5, 0), (0, 7)] {
        let w = hooked(&c, op_index(&c, "rename", 0) + 1, |s: &mut State| {
            s.files.get_mut(b"/w/b.h".as_slice()).unwrap().ino += 1;
        });
        for n in w.0.borrow_mut().files.values_mut() {
            (n.mtime, n.ctime) = ((1, m), (1, ct));
        }
        let want = Err(WriteError::Refused(Refused::StaleFile(b"/w/b.h".to_vec())));
        assert_eq!(run_write(&c, &w), want, "{m} {ct}");
    }
}

/// A save of /w/a.cc: other bytes, newer times.
fn save_a(s: &mut State) {
    let n = s.files.get_mut(b"/w/a.cc".as_slice()).unwrap();
    n.bytes = b"USER SAVED THIS".to_vec();
    n.mtime.0 += 1;
    n.ctime.0 += 1;
}

/// The index of the `nth` operation named `op` in `run` over a healthy copy
/// of `w`.
fn index_in(w: &World, op: &str, nth: usize, run: impl Fn(&World)) -> u64 {
    let probe = w.healthy();
    run(&probe);
    let log = &probe.0.borrow().log;
    log.iter().enumerate().filter(|(_, (o, _))| *o == op).nth(nth).unwrap().0 as u64
}

// §7.3 recovery: a file holding the post-image that changes after
// `restore` read it (just before its rename) is reported and kept, not
// overwritten; the journal is kept. Every field of the identity counts,
// not only the bytes.
#[test]
fn s7_3_recover_keeps_a_save_made_before_its_rename() {
    let c = case();
    let start = after_commit(&c);
    // Just before the rename and the re-check (`stat`) that precedes it.
    let at = index_in(&start, "rename", 0, |w| {
        recover(&own_w(), w, w, w).unwrap().unwrap();
    }) - 1;
    let changes: [(&str, fn(&mut State)); 7] = [
        ("save", save_a),
        ("dev", |s| s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().dev += 1),
        ("ino", |s| s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().ino += 1),
        ("mtime", |s| s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().mtime.0 += 1),
        ("mtime ns", |s| s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().mtime.1 += 1),
        ("ctime", |s| s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().ctime.0 += 1),
        ("ctime ns", |s| s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().ctime.1 += 1),
    ];
    for (what, change) in changes {
        let w = start.healthy();
        w.0.borrow_mut().hook = Some((at, Rc::new(change)));
        let got = recover(&own_w(), &w, &w, &w).unwrap().unwrap();
        assert_eq!(got.reported, vec![b"/w/a.cc".to_vec()], "{what}");
        assert!(!got.restored.contains(&b"/w/a.cc".to_vec()), "{what}");
        assert!(w.0.borrow().journal.is_some(), "{what}");
        let want = if what == "save" { &b"USER SAVED THIS"[..] } else { &c.post[0].1[..] };
        assert_eq!(w.bytes(b"/w/a.cc").unwrap(), want, "{what}");
        // The other files are restored; the pre-image is left beside it.
        assert_eq!(w.bytes(b"/w/b.h").unwrap(), c.pre[1].1, "{what}");
        assert_eq!(w.bytes(&staged_name(b"/w/a.cc", c.id)).unwrap(), c.pre[0].1, "{what}");
    }
}

// §7.3 step 5: the rollback after a commit refusal is the same restore: a
// save of a renamed file made just before the rollback's rename is kept,
// reported, and the journal left.
#[test]
fn s7_3_rollback_keeps_a_save_made_before_its_rename() {
    let c = case();
    // The refusal: the staged file of /w/b.h touched before its re-check.
    let refuse_at = op_index(&c, "rename", 0) + 1;
    let sb = staged_name(b"/w/b.h", c.id);
    let touch: Hook = Rc::new(move |s: &mut State| s.files.get_mut(&sb).unwrap().mtime.1 += 1);
    let refused = c.world.healthy();
    refused.0.borrow_mut().hook = Some((refuse_at, touch.clone()));
    // Before the rollback's rename of /w/a.cc (the second rename of that
    // run) and the re-check that precedes it.
    let at = index_in(&refused, "rename", 1, |w| {
        w.0.borrow_mut().hook = Some((refuse_at, touch.clone()));
        let got = run_write(&c, w);
        assert_eq!(got, Err(WriteError::Refused(Refused::StaleFile(staged_name(b"/w/b.h", c.id)))));
    }) - 1;
    let w = c.world.healthy();
    w.0.borrow_mut().hook = Some((refuse_at, touch));
    w.0.borrow_mut().later = Some((at, Rc::new(save_a)));
    assert!(matches!(run_write(&c, &w), Err(WriteError::Io { journal_left: true, .. })));
    assert!(w.0.borrow().journal.is_some());
    assert_eq!(w.bytes(b"/w/a.cc").unwrap(), b"USER SAVED THIS");
    assert_eq!(w.bytes(b"/w/b.h").unwrap(), c.pre[1].1);
}

// §7.3 recovery: a file that is reported by its `stat` (here: under a
// directory that is a link, which `DiskTree` gives as not regular) has
// nothing removed under it, its staged name included.
#[test]
fn s7_3_recover_removes_nothing_under_a_reported_path() {
    let c = case();
    // Interrupted before the first rename: every staged file exists.
    let w = c.world.with_fault(op_index(&c, "rename", 0), Mode::Lose);
    let _ = run_write(&c, &w);
    let w = w.healthy();
    w.0.borrow_mut().files.get_mut(b"/w/sub/c.sh".as_slice()).unwrap().regular = false;
    let got = recover(&own_w(), &w, &w, &w).unwrap().unwrap();
    assert_eq!(got.reported, vec![b"/w/sub/c.sh".to_vec()]);
    let sc = staged_name(b"/w/sub/c.sh", c.id);
    assert!(w.0.borrow().log.iter().all(|(op, p)| *op != "remove" || *p != sc));
    assert!(w.bytes(&sc).is_some());
    assert!(w.bytes(&staged_name(b"/w/a.cc", c.id)).is_none());
}

// §7.3 step 5: a rollback that reports a file (changed by someone after
// its rename) leaves the journal: `journal_left` is true.
#[test]
fn s7_3_rollback_with_a_reported_file_leaves_the_journal() {
    let c = case();
    let at = op_index(&c, "rename", 1);
    let w = c.world.with_fault(at, Mode::Fail);
    w.0.borrow_mut().hook = Some((
        at,
        Rc::new(|s: &mut State| {
            s.files.get_mut(b"/w/a.cc".as_slice()).unwrap().bytes = b"theirs".to_vec();
        }),
    ));
    assert!(matches!(run_write(&c, &w), Err(WriteError::Io { journal_left: true, .. })));
    assert!(w.0.borrow().journal.is_some());
    assert_eq!(w.bytes(b"/w/a.cc").unwrap(), b"theirs");
}

// §7.3 step 4: a refusal whose undo fails leaves the journal
// (`journal_left` true); an undo that runs syncs both directories before
// it removes the journal.
#[test]
fn s7_3_undo_of_a_refusal() {
    let c = case();
    let after = op_index(&c, "create", 2) + 1;
    let stale = |s: &mut State| {
        s.files.get_mut(b"/w/b.h".as_slice()).unwrap().bytes = b"someone's".to_vec();
    };
    let w = hooked(&c, after, stale);
    assert_eq!(run_write(&c, &w), Err(WriteError::Refused(Refused::StaleFile(b"/w/b.h".to_vec()))));
    let log = w.0.borrow().log.clone();
    let first_remove = log.iter().position(|(o, _)| *o == "remove").unwrap();
    let gone = log.iter().position(|(o, _)| *o == "unjournal").unwrap();
    let synced: BTreeSet<Vec<u8>> = log[first_remove..gone]
        .iter()
        .filter(|(o, _)| *o == "sync_dir")
        .map(|(_, p)| p.clone())
        .collect();
    assert_eq!(synced, BTreeSet::from([b"/w".to_vec(), b"/w/sub".to_vec()]));
    for target in [first_remove, gone] {
        let w = c.world.with_fault(target as u64, Mode::Fail);
        w.0.borrow_mut().hook = Some((after, Rc::new(stale)));
        assert!(
            matches!(run_write(&c, &w), Err(WriteError::Io { journal_left: true, .. })),
            "fault at {}",
            log[target].0
        );
        assert!(w.0.borrow().journal.is_some());
    }
}

// §7.3 and §7.4: `recover` takes the write's ownership and refuses, before
// touching anything, a journal naming a path it does not own (outside it,
// or under a readonly entry); the journal is kept.
#[test]
fn s7_3_recover_refuses_a_journal_outside_its_ownership() {
    let c = case();
    let start = after_commit(&c);
    let narrow = owned(&["/w/sub"]);
    let carved = Ownership { owned: vec![b"/w".to_vec()], readonly: vec![b"/w/b.h".to_vec()] };
    for (own, path) in [(narrow, b"/w/a.cc".to_vec()), (carved, b"/w/b.h".to_vec())] {
        let w = start.healthy();
        assert_eq!(
            recover(&own, &w, &w, &w),
            Err(WriteError::Refused(Refused::JournalNotOwned(path)))
        );
        let s = w.0.borrow();
        assert!(s.journal.is_some());
        assert!(s.log.iter().all(|(op, _)| *op == "journal"), "{:?}", s.log);
    }
}

// §7.3: an ownership that is not canonical (`""` would cover every path)
// or has no owned entry is `Invalid`, for `write` and for `recover`.
#[test]
fn s7_3_ownership_is_validated() {
    let c = case();
    let bad = [
        Ownership { owned: vec![b"".to_vec()], readonly: vec![] },
        Ownership { owned: vec![b"w".to_vec()], readonly: vec![] },
        Ownership { owned: vec![b"/w/".to_vec()], readonly: vec![] },
        Ownership { owned: vec![], readonly: vec![] },
        Ownership { owned: vec![b"/w".to_vec()], readonly: vec![b"/w/../x".to_vec()] },
        Ownership { owned: vec![b"/w".to_vec()], readonly: vec![b"/w".to_vec()] },
    ];
    for own in bad {
        let w = c.world.healthy();
        assert!(
            matches!(write(&c.overlay, c.id, &own, &w, &w, &w), Err(WriteError::Invalid(_))),
            "{own:?}"
        );
        assert!(matches!(recover(&own, &w, &w, &w), Err(WriteError::Invalid(_))), "{own:?}");
        assert_eq!(w.0.borrow().ops, 0);
    }
}

// §7.3 step 2: the journal is published exclusively; a journal that
// appears after step 0's read (another write) is the unfinished-write
// refusal, and it is not removed.
#[test]
fn s7_3_publish_is_exclusive() {
    let c = case();
    let at = op_index(&c, "publish", 0);
    let w = hooked(&c, at, |s: &mut State| s.journal = Some(b"theirs".to_vec()));
    assert!(matches!(run_write(&c, &w), Err(WriteError::Journal(_))));
    let s = w.0.borrow();
    assert_eq!(s.journal.as_deref(), Some(&b"theirs"[..]));
    assert!(s.log.iter().all(|(op, _)| !matches!(*op, "create" | "rename" | "unjournal")));
}

// §7.3: an unfinished write blocks another; a malformed overlay or a wrong
// id is refused; an overlay with no change does nothing at all.
#[test]
fn s7_3_write_refuses_bad_input() {
    let c = case();
    let w = c.world.healthy();
    w.0.borrow_mut().journal = Some(b"x".to_vec());
    assert!(matches!(run_write(&c, &w), Err(WriteError::Journal(_))));
    assert_eq!(w.0.borrow().journal.as_deref(), Some(&b"x"[..]));
    let w = c.world.healthy();
    assert!(matches!(
        write(&c.overlay, Digest([0; 32]), &owned(&["/w"]), &w, &w, &w),
        Err(WriteError::Invalid(_))
    ));
    let mut bad = c.overlay.clone();
    bad.files.reverse();
    assert!(matches!(write(&bad, c.id, &owned(&["/w"]), &w, &w, &w), Err(WriteError::Invalid(_))));
    let only_same = Overlay {
        files: vec![c.overlay.files.iter().find(|f| f.path == SAME).unwrap().clone()],
        ..c.overlay.clone()
    };
    let id = formats::content_id(&only_same.to_bytes()).unwrap();
    assert_eq!(write(&only_same, id, &owned(&["/w"]), &w, &w, &w), Ok(0));
    assert_eq!(w.0.borrow().ops, 0);
}

// §7.3: `recover` refuses a malformed journal and touches nothing.
#[test]
fn s7_3_recover_refuses_a_malformed_journal() {
    let c = case();
    let w = c.world.healthy();
    for bytes in [&b""[..], b"garbage", &[0u8; 33]] {
        w.0.borrow_mut().journal = Some(bytes.to_vec());
        assert!(matches!(recover(&own_w(), &w, &w, &w), Err(WriteError::Journal(_))));
    }
    assert!(w.0.borrow().log.iter().all(|(op, _)| *op == "journal"));
}

// The journal's record (§7.3 step 2): round trip, and every rule of its
// reader (spec/landed/edits/WP-063.md §4).
#[test]
fn s7_3_journal_record() {
    let id = Digest([0xab; 32]);
    assert_eq!(staged_name(b"/a/b.cc", id), b"/a/.b.cc.slip-abababababababab");
    assert_eq!(staged_name(b"/x", id), b"/.x.slip-abababababababab");
    let entry = |p: &[u8]| JournalEntry {
        path: p.to_vec(),
        staged: staged_name(p, id),
        pre: content(b"1"),
        post: content(b"2"),
    };
    let j = Journal { overlay: id, files: vec![entry(b"/a/b"), entry(b"/a/c")] };
    let bytes = j.to_bytes();
    assert_eq!(Journal::from_bytes(&bytes), Ok(j.clone()));
    assert!(Journal::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    assert!(Journal::from_bytes(&[bytes.clone(), vec![0]].concat()).is_err());
    let bad = |f: &dyn Fn(&mut Journal), what: &str| {
        let mut x = j.clone();
        f(&mut x);
        let e = Journal::from_bytes(&x.to_bytes()).unwrap_err();
        assert!(e.contains(what), "{e}");
    };
    bad(&|x| x.files.clear(), "names no file");
    bad(&|x| x.files[1].path = b"/a/../c".to_vec(), "canonical");
    bad(&|x| x.files[1].path = b"/".to_vec(), "canonical");
    bad(&|x| x.files.reverse(), "ascending");
    bad(&|x| x.files[0].staged = b"/etc/passwd".to_vec(), "staged name");
    bad(&|x| x.files[0].post = x.files[0].pre, "pre and post");
    bad(&|x| x.files[1] = x.files[0].clone(), "ascending");
    // Untrusted bytes: every prefix and every byte changed, never a panic;
    // a change is either refused or read as a journal that writes back to
    // the same bytes.
    for n in 0..bytes.len() {
        assert!(Journal::from_bytes(&bytes[..n]).is_err());
    }
    for i in 0..bytes.len() {
        for v in [0u8, 1, 0x7f, 0x80, 0xff, bytes[i] ^ 1] {
            let mut x = bytes.clone();
            x[i] = v;
            if let Ok(r) = Journal::from_bytes(&x) {
                assert_eq!(r.to_bytes(), x);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// On disk: DiskTree, with blobs and a journal in a directory.

mod disk {
    use super::*;
    use slip_apply::fs::DiskTree;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, symlink};
    use std::path::{Path, PathBuf};

    pub struct Store(pub PathBuf);

    impl Blobs for Store {
        fn get(&self, id: Digest) -> io::Result<Vec<u8>> {
            std::fs::read(self.0.join(id.to_string()))
        }
        fn put(&self, bytes: &[u8]) -> io::Result<Digest> {
            let d = content_digest(bytes);
            std::fs::write(self.0.join(d.to_string()), bytes)?;
            Ok(d)
        }
    }

    impl JournalStore for Store {
        fn publish(&self, bytes: &[u8]) -> io::Result<()> {
            // Exclusive: a hard link fails if the journal exists.
            let tmp = self.0.join("write.journal.tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::File::open(&tmp)?.sync_all()?;
            let linked = std::fs::hard_link(&tmp, self.0.join("write.journal"));
            std::fs::remove_file(&tmp)?;
            linked?;
            std::fs::File::open(&self.0)?.sync_all()
        }
        fn read(&self) -> io::Result<Option<Vec<u8>>> {
            match std::fs::read(self.0.join("write.journal")) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            }
        }
        fn remove(&self) -> io::Result<()> {
            std::fs::remove_file(self.0.join("write.journal"))
        }
    }

    /// `DiskTree` that loses power at its n-th operation (`None`: never).
    pub struct Flaky(pub RefCell<(u64, Option<u64>)>);

    impl Flaky {
        fn go(&self) -> io::Result<()> {
            let mut s = self.0.borrow_mut();
            s.0 += 1;
            match s.1 {
                Some(n) if s.0 > n => Err(io::Error::other("no power")),
                _ => Ok(()),
            }
        }
    }

    impl Tree for Flaky {
        fn stat(&self, p: &[u8]) -> io::Result<Option<Stat>> {
            self.go()?;
            DiskTree.stat(p)
        }
        fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
            self.go()?;
            DiskTree.read(p)
        }
        fn create(&self, p: &[u8], mode: u32, b: &[u8]) -> io::Result<()> {
            self.go()?;
            DiskTree.create(p, mode, b)
        }
        fn rename(&self, a: &[u8], b: &[u8]) -> io::Result<()> {
            self.go()?;
            DiskTree.rename(a, b)
        }
        fn remove(&self, p: &[u8]) -> io::Result<()> {
            self.go()?;
            DiskTree.remove(p)
        }
        fn sync_dir(&self, d: &[u8]) -> io::Result<()> {
            self.go()?;
            DiskTree.sync_dir(d)
        }
        fn names(&self, d: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
            DiskTree.names(d)
        }
    }

    pub fn root(name: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp063-write-{name}-{}", std::process::id()));
        if d.exists() {
            common::remove_tree(&d);
        }
        std::fs::create_dir_all(d.join("tree/sub")).unwrap();
        std::fs::create_dir_all(d.join("store")).unwrap();
        std::fs::canonicalize(&d).unwrap()
    }

    pub fn bytes(p: &Path) -> Vec<u8> {
        p.as_os_str().as_bytes().to_vec()
    }

    pub const FILES: [(&str, &[u8], &[u8], u32); 3] = [
        // 0o666: wider than the umask lets a create make it.
        ("tree/a.cc", b"int a;\r\nint b;", b"int alpha;\r\nint b;", 0o666),
        // The setuid bit, beyond the nine permission bits.
        ("tree/run.sh", b"#!/bin/sh\necho a\n", b"#!/bin/sh\necho alpha\n", 0o4755),
        ("tree/sub/c.h", b"\xff\xfe a", b"\xff\xfe alpha", 0o600),
    ];

    /// Writes the pre-image, the post blobs; returns the overlay and its id.
    pub fn setup(r: &Path) -> (Overlay, Digest) {
        let mut files = Vec::new();
        for (p, a, b, mode) in FILES {
            let at = r.join(p);
            std::fs::write(&at, a).unwrap();
            std::fs::set_permissions(&at, std::fs::Permissions::from_mode(mode)).unwrap();
            Store(r.join("store")).put(b).unwrap();
            files.push(OverlayFile { path: bytes(&at), pre: content(a), post: content(b) });
        }
        files.sort_by(|x, y| x.path.cmp(&y.path));
        let o = Overlay { edits: Digest([1; 32]), files, spellings: vec![] };
        let id = formats::content_id(&o.to_bytes()).unwrap();
        (o, id)
    }

    pub fn own(r: &Path) -> Ownership {
        Ownership { owned: vec![bytes(&r.join("tree"))], readonly: vec![] }
    }

    pub fn mode(p: &Path) -> u32 {
        std::fs::symlink_metadata(p).unwrap().mode() & 0o7777
    }

    pub fn link(target: &Path, at: &Path) {
        symlink(target, at).unwrap();
    }

    /// `DiskTree` that logs its operations and runs a closure just before
    /// the one of index `at`.
    pub struct Hooked {
        pub log: RefCell<Vec<&'static str>>,
        pub hook: RefCell<Option<(usize, Box<dyn Fn()>)>>,
    }

    impl Hooked {
        pub fn new(hook: Option<(usize, Box<dyn Fn()>)>) -> Hooked {
            Hooked { log: RefCell::new(Vec::new()), hook: RefCell::new(hook) }
        }
        fn go(&self, op: &'static str) {
            let n = {
                let mut log = self.log.borrow_mut();
                log.push(op);
                log.len() - 1
            };
            if let Some((at, f)) = &*self.hook.borrow()
                && *at == n
            {
                f();
            }
        }
        /// The index of the `nth` operation named `op`.
        pub fn index(&self, op: &str, nth: usize) -> usize {
            self.log.borrow().iter().enumerate().filter(|(_, o)| **o == op).nth(nth).unwrap().0
        }
    }

    impl Tree for Hooked {
        fn stat(&self, p: &[u8]) -> io::Result<Option<Stat>> {
            self.go("stat");
            DiskTree.stat(p)
        }
        fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
            self.go("read");
            DiskTree.read(p)
        }
        fn create(&self, p: &[u8], mode: u32, b: &[u8]) -> io::Result<()> {
            self.go("create");
            DiskTree.create(p, mode, b)
        }
        fn rename(&self, a: &[u8], b: &[u8]) -> io::Result<()> {
            self.go("rename");
            DiskTree.rename(a, b)
        }
        fn remove(&self, p: &[u8]) -> io::Result<()> {
            self.go("remove");
            DiskTree.remove(p)
        }
        fn sync_dir(&self, d: &[u8]) -> io::Result<()> {
            self.go("sync_dir");
            DiskTree.sync_dir(d)
        }
        fn names(&self, d: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
            DiskTree.names(d)
        }
    }

    /// The operation indices of a clean write over a fresh tree.
    pub fn clean_log(name: &str) -> Hooked {
        let r = root(name);
        let (o, id) = setup(&r);
        let store = Store(r.join("store"));
        let h = Hooked::new(None);
        assert_eq!(write(&o, id, &own(&r), &h, &store, &store), Ok(3));
        common::remove_tree(&r);
        h
    }
}

// §7.3 over the disk: the files replaced, permission bits and the
// executable bit kept, bytes exact (CRLF, no final newline, invalid
// UTF-8); a symbolic link to an overlay file still points at the replaced
// file; no staged file or journal remains.
#[test]
fn s7_3_disk_write() {
    let r = disk::root("ok");
    let (o, id) = disk::setup(&r);
    disk::link(&r.join("tree/a.cc"), &r.join("tree/link.cc"));
    let store = disk::Store(r.join("store"));
    assert_eq!(write(&o, id, &disk::own(&r), &slip_apply::fs::DiskTree, &store, &store), Ok(3));
    for (p, _, b, mode) in disk::FILES {
        assert_eq!(std::fs::read(r.join(p)).unwrap(), b);
        assert_eq!(disk::mode(&r.join(p)), mode, "{p}");
    }
    let link = r.join("tree/link.cc");
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), r.join("tree/a.cc"));
    assert_eq!(std::fs::read(&link).unwrap(), disk::FILES[0].2);
    let names: Vec<_> =
        std::fs::read_dir(r.join("tree")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names.len(), 4, "{names:?}");
    assert!(!r.join("store/write.journal").exists());
    assert_eq!(recover(&disk::own(&r), &slip_apply::fs::DiskTree, &store, &store), Ok(None));
    common::remove_tree(&r);
}

// §7.3 over the disk: a write that loses power after each of its first
// operations, then `recover` over the disk: the pre-image, modes kept.
#[test]
fn s7_3_disk_interrupted_write_and_recover() {
    let probe = disk::root("probe");
    let (o, id) = disk::setup(&probe);
    let store = disk::Store(probe.join("store"));
    let flaky = disk::Flaky(RefCell::new((0, None)));
    write(&o, id, &disk::own(&probe), &flaky, &store, &store).unwrap();
    let n = flaky.0.borrow().0;
    common::remove_tree(&probe);
    let mut journals = 0;
    for at in 0..n {
        let r = disk::root(&format!("cut{at}"));
        let (o, id) = disk::setup(&r);
        let store = disk::Store(r.join("store"));
        let flaky = disk::Flaky(RefCell::new((0, Some(at))));
        assert!(write(&o, id, &disk::own(&r), &flaky, &store, &store).is_err());
        if r.join("store/write.journal").exists() {
            journals += 1;
            let got = recover(&disk::own(&r), &slip_apply::fs::DiskTree, &store, &store)
                .unwrap()
                .unwrap();
            assert!(got.reported.is_empty());
        }
        for (p, a, _, mode) in disk::FILES {
            assert_eq!(std::fs::read(r.join(p)).unwrap(), a, "cut at {at}: {p}");
            assert_eq!(disk::mode(&r.join(p)), mode);
        }
        assert_eq!(std::fs::read_dir(r.join("tree")).unwrap().count(), 3, "cut at {at}");
        assert_eq!(std::fs::read_dir(r.join("tree/sub")).unwrap().count(), 1, "cut at {at}");
        common::remove_tree(&r);
    }
    assert!(journals > 10, "{journals} of {n}");
}

// §7.3 over the disk: DiskTree's refusals: a symbolic link at the path, a
// path through a linked directory, a hard link, a read-only directory.
#[test]
fn s7_3_disk_refusals() {
    let r = disk::root("refuse");
    let (o, id) = disk::setup(&r);
    let store = disk::Store(r.join("store"));
    let tree = slip_apply::fs::DiskTree;
    let try_write = || write(&o, id, &disk::own(&r), &tree, &store, &store);
    let a = r.join("tree/a.cc");
    // A hard link.
    std::fs::hard_link(&a, r.join("tree/a2.cc")).unwrap();
    assert_eq!(try_write(), Err(WriteError::Refused(Refused::HardLinked(disk::bytes(&a)))));
    std::fs::remove_file(r.join("tree/a2.cc")).unwrap();
    // The path is a symbolic link to a file of the same bytes.
    std::fs::rename(&a, r.join("a.real")).unwrap();
    disk::link(&r.join("a.real"), &a);
    assert_eq!(try_write(), Err(WriteError::Refused(Refused::NotRegular(disk::bytes(&a)))));
    std::fs::remove_file(&a).unwrap();
    std::fs::rename(r.join("a.real"), &a).unwrap();
    // A directory on the path is a link.
    let sub = r.join("tree/sub");
    std::fs::rename(&sub, r.join("sub.real")).unwrap();
    disk::link(&r.join("sub.real"), &sub);
    let c = disk::bytes(&r.join("tree/sub/c.h"));
    assert_eq!(try_write(), Err(WriteError::Refused(Refused::NotRegular(c.clone()))));
    std::fs::remove_file(&sub).unwrap();
    std::fs::rename(r.join("sub.real"), &sub).unwrap();
    // A read-only directory (our own, under the target directory).
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
    assert_eq!(try_write(), Err(WriteError::Refused(Refused::NotWritable(c))));
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Nothing was written by any of them.
    for (p, pre, _, _) in disk::FILES {
        assert_eq!(std::fs::read(r.join(p)).unwrap(), pre);
    }
    assert!(!r.join("store/write.journal").exists());
    // The directory renames above give the files under them new inode
    // numbers on this host's volume (a FUSE-backed host mount), possibly
    // between step 4 and the renames: with nanosecond times the re-check
    // does not count that as a change (§7.3 step 5).
    assert_eq!(try_write(), Ok(3));
    common::remove_tree(&r);
}

// §7.3 step 3 on disk: the staged name of /tree/a.cc appears between
// step 1 and its create, as a file and as a link to a victim: the create
// is exclusive, nothing is written through it, it is not removed, the
// write fails with nothing left.
#[test]
fn s7_3_disk_staged_name_appears_before_the_create() {
    let probe = disk::clean_log("excl-probe");
    let at = probe.index("create", 0);
    for as_link in [false, true] {
        let r = disk::root(if as_link { "excl-link" } else { "excl-file" });
        let (o, id) = disk::setup(&r);
        let store = disk::Store(r.join("store"));
        let a = r.join(disk::FILES[0].0);
        let staged = std::path::PathBuf::from(
            <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(&staged_name(
                &disk::bytes(&a),
                id,
            )),
        );
        let victim = r.join("victim");
        std::fs::write(&victim, b"precious").unwrap();
        std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (s2, v2) = (staged.clone(), victim.clone());
        let h = disk::Hooked::new(Some((
            at,
            Box::new(move || {
                if as_link {
                    disk::link(&v2, &s2);
                } else {
                    std::fs::write(&s2, b"theirs").unwrap();
                }
            }),
        )));
        let got = write(&o, id, &disk::own(&r), &h, &store, &store);
        assert!(matches!(got, Err(WriteError::Io { journal_left: false, .. })), "{got:?}");
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
        assert_eq!(disk::mode(&victim), 0o600);
        let m = std::fs::symlink_metadata(&staged).unwrap();
        assert_eq!(m.file_type().is_symlink(), as_link);
        if !as_link {
            assert_eq!(std::fs::read(&staged).unwrap(), b"theirs");
        }
        for (p, pre, _, mode) in disk::FILES {
            assert_eq!(std::fs::read(r.join(p)).unwrap(), pre);
            assert_eq!(disk::mode(&r.join(p)), mode);
        }
        assert!(!r.join("store/write.journal").exists());
        common::remove_tree(&r);
    }
}

// §7.3 step 5 on disk: the file rewritten in place with bytes of the same
// size between step 4 and its rename (an editor's save) is found by its
// times: refused, the save kept and reported, the journal left; the other
// file already renamed is restored.
#[test]
fn s7_3_disk_save_in_place_before_the_rename() {
    let probe = disk::clean_log("save-probe");
    // Just after the first rename: before the re-check of the second file.
    let at = probe.index("rename", 0) + 1;
    let r = disk::root("save");
    let (o, id) = disk::setup(&r);
    let store = disk::Store(r.join("store"));
    let second = r.join(disk::FILES[1].0);
    let saved: Vec<u8> = disk::FILES[1].1.iter().map(u8::to_ascii_uppercase).collect();
    assert_eq!(saved.len(), disk::FILES[1].1.len());
    let (p2, b2) = (second.clone(), saved.clone());
    let h = disk::Hooked::new(Some((
        at,
        Box::new(move || {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new().write(true).open(&p2).unwrap();
            f.write_all(&b2).unwrap();
        }),
    )));
    let got = write(&o, id, &disk::own(&r), &h, &store, &store);
    assert!(matches!(got, Err(WriteError::Io { journal_left: true, .. })), "{got:?}");
    assert_eq!(std::fs::read(&second).unwrap(), saved);
    assert_eq!(std::fs::read(r.join(disk::FILES[0].0)).unwrap(), disk::FILES[0].1);
    assert_eq!(std::fs::read(r.join(disk::FILES[2].0)).unwrap(), disk::FILES[2].1);
    assert!(r.join("store/write.journal").exists());
    let again =
        recover(&disk::own(&r), &slip_apply::fs::DiskTree, &store, &store).unwrap().unwrap();
    assert_eq!(again.reported, vec![disk::bytes(&second)]);
    common::remove_tree(&r);
}

// §7.3 on disk: `DiskTree::create` with its staged name swapped for a link
// to a victim by another thread, as often as the race can be won: the
// victim's bytes and mode never change, and a create that found the swap
// fails. The race is not won every run; the test counts the swaps seen.
#[test]
fn s7_3_disk_create_does_not_follow_a_swapped_name() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let r = disk::root("race");
    let victim = r.join("victim");
    std::fs::write(&victim, b"precious").unwrap();
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
    let staged = r.join("tree/.f.slip-0000000000000000");
    let stop = Arc::new(AtomicBool::new(false));
    let (s2, st2, v2) = (stop.clone(), staged.clone(), victim.clone());
    let attacker = std::thread::spawn(move || {
        let tmp = st2.with_extension("lnk");
        while !s2.load(Ordering::Relaxed) {
            if std::fs::symlink_metadata(&st2).is_ok_and(|m| m.file_type().is_file()) {
                let _ = std::fs::remove_file(&tmp);
                let _ = std::os::unix::fs::symlink(&v2, &tmp);
                let _ = std::fs::rename(&tmp, &st2);
            }
        }
    });
    let (mut swapped, mut failed, mut caught) = (0, 0, 0);
    // A long enough write that the swap often lands inside the create.
    let post = vec![b'p'; 1 << 18];
    for _ in 0..600 {
        let _ = std::fs::remove_file(&staged);
        let got = slip_apply::fs::DiskTree.create(&disk::bytes(&staged), 0o4755, &post);
        assert_eq!(disk::mode(&victim), 0o600);
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
        let link = std::fs::symlink_metadata(&staged).is_ok_and(|m| m.file_type().is_symlink());
        match &got {
            Err(e) if e.to_string().contains("replaced while") => caught += 1,
            Err(_) => failed += 1,
            Ok(()) => {}
        }
        if link && got.is_ok() {
            // The swap came after the create had finished.
            swapped += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    attacker.join().unwrap();
    eprintln!(
        "race: {caught} swaps caught, {failed} creates failed (the name taken), {swapped} swaps after a create"
    );
    common::remove_tree(&r);
}

// ---------------------------------------------------------------------------
// One spelling (§2.1): a path is written only under the spelling its
// directories list.

/// A tree that finds a name in any case, as a case-insensitive volume does:
/// `stat` and `read` look a path up with its case folded, `names` lists
/// the names as they were made. It only answers questions; a write
/// through it fails.
struct Folding(BTreeMap<Vec<u8>, (Vec<u8>, Vec<u8>)>);

impl Folding {
    fn new(files: &[(&str, &[u8])]) -> Folding {
        Folding(
            files
                .iter()
                .map(|(p, b)| (p.to_lowercase().into_bytes(), (p.as_bytes().to_vec(), b.to_vec())))
                .collect(),
        )
    }
    fn get(&self, p: &[u8]) -> Option<&(Vec<u8>, Vec<u8>)> {
        self.0.get(&p.to_ascii_lowercase())
    }
}

impl Tree for Folding {
    fn stat(&self, p: &[u8]) -> io::Result<Option<Stat>> {
        Ok(self.get(p).map(|(_, b)| Stat {
            regular: true,
            links: 1,
            mode: 0o644,
            dir_writable: true,
            uid: 0,
            gid: 0,
            dev: 1,
            ino: 1,
            size: b.len() as u64,
            mtime: (1, 1),
            ctime: (1, 1),
        }))
    }
    fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
        self.get(p).map(|(_, b)| b.clone()).ok_or_else(|| err(io::ErrorKind::NotFound, "none"))
    }
    fn create(&self, _: &[u8], _: u32, _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn rename(&self, _: &[u8], _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn remove(&self, _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn sync_dir(&self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn names(&self, d: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        let prefix = if d == b"/" { b"/".to_vec() } else { [d, b"/"].concat() };
        let mut names = BTreeSet::new();
        for (p, _) in self.0.values() {
            if p.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase()) {
                let rest = &p[prefix.len()..];
                let end = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
                names.insert(rest[..end].to_vec());
            }
        }
        Ok(if names.is_empty() { None } else { Some(names.into_iter().collect()) })
    }
}

fn one_file(path: &str, pre: &[u8], post: &[u8]) -> (Overlay, Digest) {
    let o = Overlay {
        edits: Digest([1; 32]),
        files: vec![OverlayFile { path: path.as_bytes().to_vec(), pre: c(pre), post: c(post) }],
        spellings: vec![],
    };
    let id = formats::content_id(&o.to_bytes()).unwrap();
    (o, id)
}

fn c(b: &[u8]) -> Content {
    Content { size: b.len() as u64, content: content_digest(b) }
}

fn vendor_ownership() -> Ownership {
    Ownership { owned: vec![b"/w".to_vec()], readonly: vec![b"/w/vendor".to_vec()] }
}

/// The reviewer's case: `/w` owned, `/w/vendor` read-only, the overlay
/// names `/w/Vendor/x.h`: `write`, `check_owned` and `recover` refuse it,
/// `NotCanonical`, naming the spelling on disk, before any ownership
/// decision; the exact spelling is `NotOwned`.
#[test]
fn s2_1_another_case_is_not_canonical() {
    let tree = Folding::new(&[("/w/vendor/x.h", b"int v;\n"), ("/w/a.cc", b"a")]);
    let w = World(Rc::new(RefCell::new(State::default())));
    let want = Refused::NotCanonical {
        path: b"/w/Vendor/x.h".to_vec(),
        on_disk: Some(b"/w/vendor/x.h".to_vec()),
    };
    let (o, id) = one_file("/w/Vendor/x.h", b"int v;\n", b"int w;\n");
    assert_eq!(
        write(&o, id, &vendor_ownership(), &tree, &w, &w),
        Err(WriteError::Refused(want.clone()))
    );
    assert_eq!(
        slip_apply::check_owned(&o, &vendor_ownership(), &tree),
        Err(WriteError::Refused(want.clone()))
    );
    assert_eq!(w.0.borrow().journal, None, "nothing journaled");
    let (o, id) = one_file("/w/vendor/x.h", b"int v;\n", b"int w;\n");
    assert_eq!(
        write(&o, id, &vendor_ownership(), &tree, &w, &w),
        Err(WriteError::Refused(Refused::NotOwned(b"/w/vendor/x.h".to_vec())))
    );
    // A file name in another case.
    let (o, id) = one_file("/w/A.cc", b"a", b"b");
    assert_eq!(
        write(&o, id, &vendor_ownership(), &tree, &w, &w),
        Err(WriteError::Refused(Refused::NotCanonical {
            path: b"/w/A.cc".to_vec(),
            on_disk: Some(b"/w/a.cc".to_vec())
        }))
    );
    // Before any ownership decision: the owned root in another case is
    // not `NotOwned` (bytes) but `NotCanonical`.
    let (o2, id2) = one_file("/W/a.cc", b"a", b"b");
    assert_eq!(
        write(&o2, id2, &vendor_ownership(), &tree, &w, &w),
        Err(WriteError::Refused(Refused::NotCanonical {
            path: b"/W/a.cc".to_vec(),
            on_disk: Some(b"/w/a.cc".to_vec())
        }))
    );
    // `recover` checks every path of its journal first, and keeps it.
    let j = Journal::of_write(&o, id);
    w.0.borrow_mut().journal = Some(j.to_bytes());
    assert!(matches!(
        recover(&vendor_ownership(), &tree, &w, &w),
        Err(WriteError::Refused(Refused::NotCanonical { .. }))
    ));
    assert!(w.0.borrow().journal.is_some());
}

/// A path that finds nothing is not a spelling question: the checks
/// after the spelling find it missing (`StaleFile`).
#[test]
fn s2_1_a_missing_path_is_left_to_the_checks_after() {
    let tree = Folding::new(&[("/w/a.cc", b"a")]);
    let w = World(Rc::new(RefCell::new(State::default())));
    let (o, id) = one_file("/w/sub/b.cc", b"a", b"b");
    assert!(matches!(
        write(&o, id, &vendor_ownership(), &tree, &w, &w),
        Err(WriteError::Io { .. } | WriteError::Refused(Refused::StaleFile(_)))
    ));
    let (o, id) = one_file("/w/b.cc", b"a", b"b");
    assert_eq!(
        write(&o, id, &vendor_ownership(), &tree, &w, &w),
        Err(WriteError::Refused(Refused::StaleFile(b"/w/b.cc".to_vec())))
    );
}

/// On disk: when the directory's volume finds names in any case (this
/// workspace's host mount), a directory in another case is refused;
/// on a case-sensitive volume the other case finds nothing, and the
/// write is `StaleFile`. Either way nothing is written.
#[test]
fn s2_1_disk_another_case() {
    let r = disk::root("case");
    std::fs::create_dir_all(r.join("tree/vendor")).unwrap();
    std::fs::write(r.join("tree/vendor/x.h"), "int v;\n").unwrap();
    let insensitive = r.join("tree/VENDOR/x.h").exists();
    let other = disk::bytes(&r.join("tree/Vendor/x.h"));
    let mut o = Overlay {
        edits: Digest([1; 32]),
        files: vec![OverlayFile { path: other.clone(), pre: c(b"int v;\n"), post: c(b"int w;\n") }],
        spellings: vec![],
    };
    o.files.sort_by(|a, b| a.path.cmp(&b.path));
    let id = formats::content_id(&o.to_bytes()).unwrap();
    let own = Ownership {
        owned: vec![disk::bytes(&r.join("tree"))],
        readonly: vec![disk::bytes(&r.join("tree/vendor"))],
    };
    let store = disk::Store(r.join("store"));
    let got = write(&o, id, &own, &slip_apply::fs::DiskTree, &store, &store);
    if insensitive {
        assert_eq!(
            got,
            Err(WriteError::Refused(Refused::NotCanonical {
                path: other,
                on_disk: Some(disk::bytes(&r.join("tree/vendor/x.h")))
            }))
        );
    } else {
        eprintln!(
            "s2_1_disk_another_case: {} is case-sensitive: the other case finds nothing",
            r.display()
        );
        assert_eq!(got, Err(WriteError::Refused(Refused::StaleFile(other))));
    }
    assert_eq!(std::fs::read(r.join("tree/vendor/x.h")).unwrap(), b"int v;\n");
}

// ---------------------------------------------------------------------------
// WP-147 item 1, the inventory (`spec/landed/edits/WP-147.md`, rows A1–A3):
// what the one-spelling check of WP-075 does with another Unicode
// normalisation, with a volume that answers lookups inconsistently, and
// with a directory that cannot be listed.

/// Row A1, on disk: the file is written `café.h` in NFC (`c3 a9`) and the
/// overlay names it in NFD (`65 cc 81`), which this workspace's volume
/// finds (it folds normalisation as it folds case). The listing has the
/// NFC bytes, no listed name differs from the NFD one only in ASCII case,
/// and `lstat` finds the path: refused, `NotCanonical`, nothing written.
/// On a volume that does not fold, the path finds nothing: `StaleFile`.
#[test]
fn s2_1_disk_another_normalisation() {
    let r = disk::root("nfd");
    let (nfc, nfd) = ("tree/caf\u{e9}.h", "tree/cafe\u{301}.h");
    std::fs::write(r.join(nfc), "int v;\n").unwrap();
    let folds = std::fs::symlink_metadata(r.join(nfd)).is_ok();
    let (o, id) = one_file(r.join(nfd).to_str().unwrap(), b"int v;\n", b"int w;\n");
    let own = Ownership { owned: vec![disk::bytes(&r.join("tree"))], readonly: vec![] };
    let store = disk::Store(r.join("store"));
    let got = write(&o, id, &own, &slip_apply::fs::DiskTree, &store, &store);
    if folds {
        assert!(
            matches!(&got, Err(WriteError::Refused(Refused::NotCanonical { path, .. }))
                if *path == disk::bytes(&r.join(nfd))),
            "{got:?}"
        );
    } else {
        eprintln!("s2_1_disk_another_normalisation: {} does not fold normalisation", r.display());
        assert_eq!(got, Err(WriteError::Refused(Refused::StaleFile(disk::bytes(&r.join(nfd))))));
    }
    assert_eq!(std::fs::read(r.join(nfc)).unwrap(), b"int v;\n");
    common::remove_tree(&r);
}

/// A tree that folds Unicode normalisation (`é` written NFC or NFD is
/// one name) and answers a lookup of a spelling that is not its
/// listing's "nothing" the first time and finds it after, as this
/// workspace's host mount was seen to do for case (WP-075 decision 14).
/// `names` lists the names as made.
struct Inconsistent {
    /// `(path as made, bytes)`.
    files: Vec<(Vec<u8>, Vec<u8>)>,
    asked: RefCell<BTreeSet<Vec<u8>>>,
}

/// `p` with the NFD `ö` (`o` and a combining diaeresis) made NFC.
fn nfc(p: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(p).replace("o\u{308}", "\u{f6}").into_bytes()
}

impl Inconsistent {
    fn get(&self, p: &[u8]) -> Option<&Vec<u8>> {
        let exact = self.files.iter().any(|(f, _)| f == p);
        if !exact && self.asked.borrow_mut().insert(p.to_vec()) {
            return None; // the first lookup of another spelling misses
        }
        self.files.iter().find(|(f, _)| nfc(f) == nfc(p)).map(|(_, b)| b)
    }
}

impl Tree for Inconsistent {
    fn stat(&self, p: &[u8]) -> io::Result<Option<Stat>> {
        Ok(self.get(p).map(|b| Stat {
            regular: true,
            links: 1,
            mode: 0o644,
            dir_writable: true,
            uid: 0,
            gid: 0,
            dev: 1,
            ino: 1,
            size: b.len() as u64,
            mtime: (1, 1),
            ctime: (1, 1),
        }))
    }
    fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
        self.get(p).cloned().ok_or_else(|| err(io::ErrorKind::NotFound, "none"))
    }
    fn create(&self, _: &[u8], _: u32, _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn rename(&self, _: &[u8], _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn remove(&self, _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn sync_dir(&self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn names(&self, d: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        let prefix = if d == b"/" { b"/".to_vec() } else { [d, b"/"].concat() };
        let mut names = BTreeSet::new();
        for (p, _) in &self.files {
            if let Some(rest) = p.strip_prefix(prefix.as_slice()) {
                let end = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
                names.insert(rest[..end].to_vec());
            }
        }
        Ok(if names.is_empty() { None } else { Some(names.into_iter().collect()) })
    }
}

/// Row A2: `/w` owned, `/w/vendör` (NFC) read-only, and the overlay names
/// `/w/vendör/x.h` with the `ö` in NFD. No listed name differs from the
/// NFD one in ASCII case, and the one `lstat` the check makes misses, so
/// the check passes it; `NFD` bytes are not under the `readonly` entry,
/// so `check_owned` (what `--patch` asks) says owned. Today: `Ok(())`. A
/// component that is not, byte for byte, a name of its directory's
/// listing must never pass, whatever a lookup answers.
#[test]
#[ignore = "WP-147 item 2: a component its directory does not list is refused, whatever a lookup answers"]
fn s2_1_another_normalisation_on_a_volume_that_answers_inconsistently() {
    let tree = Inconsistent {
        files: vec![
            ("/w/vend\u{f6}r/x.h".as_bytes().to_vec(), b"int v;\n".to_vec()),
            (b"/w/a.cc".to_vec(), b"a".to_vec()),
        ],
        asked: RefCell::new(BTreeSet::new()),
    };
    let own = Ownership {
        owned: vec![b"/w".to_vec()],
        readonly: vec!["/w/vend\u{f6}r".as_bytes().to_vec()],
    };
    let (o, _) = one_file("/w/vendo\u{308}r/x.h", b"int v;\n", b"int w;\n");
    let got = slip_apply::check_owned(&o, &own, &tree);
    assert!(
        matches!(
            got,
            Err(WriteError::Refused(Refused::NotCanonical { .. } | Refused::StaleFile(_)))
        ),
        "{got:?}"
    );
}

/// A case-sensitive tree (a name is found only as made) in which `/home`
/// can be searched but not listed, as a shared `/home` is for a user.
struct Unlistable(Vec<(Vec<u8>, Vec<u8>)>);

impl Tree for Unlistable {
    fn stat(&self, p: &[u8]) -> io::Result<Option<Stat>> {
        let exists = self.0.iter().any(|(f, _)| {
            f == p || (f.starts_with(p) && f.get(p.len()) == Some(&b'/'))
        });
        Ok(exists.then_some(Stat {
            regular: true,
            links: 1,
            mode: 0o644,
            dir_writable: true,
            uid: 0,
            gid: 0,
            dev: 1,
            ino: 2,
            size: 1,
            mtime: (1, 1),
            ctime: (1, 1),
        }))
    }
    fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
        let f = self.0.iter().find(|(f, _)| f == p);
        f.map(|(_, b)| b.clone()).ok_or_else(|| err(io::ErrorKind::NotFound, "none"))
    }
    fn create(&self, _: &[u8], _: u32, _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn rename(&self, _: &[u8], _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn remove(&self, _: &[u8]) -> io::Result<()> {
        Err(err(io::ErrorKind::Unsupported, "read only"))
    }
    fn sync_dir(&self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn names(&self, d: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        if d == b"/home" {
            return Err(err(io::ErrorKind::PermissionDenied, "Permission denied"));
        }
        let prefix = if d == b"/" { b"/".to_vec() } else { [d, b"/"].concat() };
        let mut names = BTreeSet::new();
        for (p, _) in &self.0 {
            if let Some(rest) = p.strip_prefix(prefix.as_slice()) {
                let end = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
                names.insert(rest[..end].to_vec());
            }
        }
        Ok(if names.is_empty() { None } else { Some(names.into_iter().collect()) })
    }
}

/// Row A3 (the package's sixth bullet): on a volume that does not fold,
/// a tree under a directory that can be searched but not listed is
/// checked without listing it: `alice` in another case reaches nothing,
/// so the name needs no listing. Today: `Err(Io)` naming `/home`
/// (status 3 from `apply`), on any volume, because the check lists every
/// directory from `/` down.
#[test]
#[ignore = "WP-147 item 2: a component the volume is seen not to fold needs no listing"]
fn s2_1_a_directory_that_cannot_be_listed() {
    let tree = Unlistable(vec![(b"/home/alice/w/a.cc".to_vec(), b"a".to_vec())]);
    let own = Ownership { owned: vec![b"/home/alice/w".to_vec()], readonly: vec![] };
    let (o, _) = one_file("/home/alice/w/a.cc", b"a", b"b");
    assert_eq!(slip_apply::check_owned(&o, &own, &tree), Ok(()));
}
