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

#[derive(Clone, Debug)]
struct Node {
    bytes: Vec<u8>,
    mode: u32,
    links: u64,
    regular: bool,
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
    fn create(&self, p: &[u8], mode: u32, bytes: &[u8], exclusive: bool) -> io::Result<()> {
        self.op("create", p, |s| {
            if exclusive && s.files.contains_key(p) {
                return Err(err(io::ErrorKind::AlreadyExists, "exists"));
            }
            if s.readonly_dirs.contains(&parent(p)) {
                return Err(err(io::ErrorKind::PermissionDenied, "read-only"));
            }
            s.files
                .insert(p.to_vec(), Node { bytes: bytes.to_vec(), mode, links: 1, regular: true });
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
            Ok(d)
        })
    }
}

impl JournalStore for World {
    fn publish(&self, bytes: &[u8]) -> io::Result<()> {
        self.op("publish", b"", |s| {
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
        s.files
            .insert(p.to_vec(), Node { bytes: a.to_vec(), mode: *mode, links: 1, regular: true });
        s.blobs.insert(content_digest(b), b.to_vec());
    }
    s.files.insert(
        OTHER.to_vec(),
        Node { bytes: b"other".to_vec(), mode: 0o644, links: 1, regular: true },
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
    let r = recover(w, w, w).expect("recover");
    let r = r.expect("a journal");
    assert!(r.reported.is_empty(), "{r:?}");
    assert!(w.0.borrow().journal.is_none());
    assert!(!image(c, w));
    let before = w.0.borrow().ops;
    assert_eq!(recover(w, w, w), Ok(None));
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
    assert_eq!(recover(&w, &w, &w), Ok(None));
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
            recover(&probe, &probe, &probe).unwrap().unwrap();
            probe.0.borrow().ops
        };
        for mode in MODES {
            for k in 0..m {
                let r = start.with_fault(k, mode);
                let _ = recover(&r, &r, &r);
                assert!(r.0.borrow().ops > k);
                visited += 1;
                image(&c, &r);
                let again = r.healthy();
                match recover(&again, &again, &again) {
                    Ok(Some(rec)) => assert!(rec.reported.is_empty(), "{rec:?}"),
                    Ok(None) => {}
                    Err(e) => panic!("{e:?}"),
                }
                assert!(again.0.borrow().journal.is_none());
                assert!(!image(&c, &again));
                assert_eq!(recover(&again, &again, &again), Ok(None));
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
                Node { bytes: vec![], mode: 0o644, links: 1, regular: true },
            );
        },
        &["/w"],
        Refused::StagedExists(staged.clone()),
    );
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
    assert_eq!(recover(&w, &w, &w), Ok(Some(want)));
    assert!(w.0.borrow().journal.is_some());
    assert_eq!(w.bytes(b"/w/b.h").unwrap(), b"edited by hand");
    assert_eq!(w.bytes(b"/w/a.cc").unwrap(), c.pre[0].1);
    let again = recover(&w, &w, &w).unwrap().unwrap();
    assert_eq!(again.reported, vec![b"/w/b.h".to_vec()]);
    assert_eq!(again.left, vec![b"/w/a.cc".to_vec(), b"/w/sub/c.sh".to_vec()]);
    // A file that is gone, or became a link, is reported too.
    w.0.borrow_mut().files.remove(b"/w/b.h".as_slice());
    assert_eq!(recover(&w, &w, &w).unwrap().unwrap().reported, vec![b"/w/b.h".to_vec()]);
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
        assert!(matches!(recover(&w, &w, &w), Err(WriteError::Journal(_))));
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
            let tmp = self.0.join("write.journal.tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::File::open(&tmp)?.sync_all()?;
            std::fs::rename(&tmp, self.0.join("write.journal"))?;
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
        fn create(&self, p: &[u8], mode: u32, b: &[u8], x: bool) -> io::Result<()> {
            self.go()?;
            DiskTree.create(p, mode, b, x)
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
    }

    pub fn root(name: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp063-write-{name}-{}", std::process::id()));
        if d.exists() {
            std::fs::remove_dir_all(&d).unwrap();
        }
        std::fs::create_dir_all(d.join("tree/sub")).unwrap();
        std::fs::create_dir_all(d.join("store")).unwrap();
        std::fs::canonicalize(&d).unwrap()
    }

    pub fn bytes(p: &Path) -> Vec<u8> {
        p.as_os_str().as_bytes().to_vec()
    }

    pub const FILES: [(&str, &[u8], &[u8], u32); 3] = [
        ("tree/a.cc", b"int a;\r\nint b;", b"int alpha;\r\nint b;", 0o640),
        ("tree/run.sh", b"#!/bin/sh\necho a\n", b"#!/bin/sh\necho alpha\n", 0o755),
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
    assert_eq!(recover(&slip_apply::fs::DiskTree, &store, &store), Ok(None));
    std::fs::remove_dir_all(&r).unwrap();
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
    std::fs::remove_dir_all(&probe).unwrap();
    let mut journals = 0;
    for at in 0..n {
        let r = disk::root(&format!("cut{at}"));
        let (o, id) = disk::setup(&r);
        let store = disk::Store(r.join("store"));
        let flaky = disk::Flaky(RefCell::new((0, Some(at))));
        assert!(write(&o, id, &disk::own(&r), &flaky, &store, &store).is_err());
        if r.join("store/write.journal").exists() {
            journals += 1;
            let got = recover(&slip_apply::fs::DiskTree, &store, &store).unwrap().unwrap();
            assert!(got.reported.is_empty());
        }
        for (p, a, _, mode) in disk::FILES {
            assert_eq!(std::fs::read(r.join(p)).unwrap(), a, "cut at {at}: {p}");
            assert_eq!(disk::mode(&r.join(p)), mode);
        }
        assert_eq!(std::fs::read_dir(r.join("tree")).unwrap().count(), 3, "cut at {at}");
        assert_eq!(std::fs::read_dir(r.join("tree/sub")).unwrap().count(), 1, "cut at {at}");
        std::fs::remove_dir_all(&r).unwrap();
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
    assert_eq!(try_write(), Ok(3));
    std::fs::remove_dir_all(&r).unwrap();
}
