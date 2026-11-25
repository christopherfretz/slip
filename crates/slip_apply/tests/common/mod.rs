//! Helpers shared by the tests of `slip_apply`: edit sets written by hand,
//! a pre-image in memory, a seeded generator.

#![allow(dead_code)]

#[path = "../../../slip_kiln/tests/common/work_root.rs"]
pub mod work_root;

/// The directory under which a test makes the trees a front end, `slip`
/// or `slip_apply` reads: under `$SLIP_TEST_ROOT` when set, else
/// `CARGO_TARGET_TMPDIR` (`spec/harness.md` §11.5).
pub fn work_root() -> std::path::PathBuf {
    work_root::work_root_or(env!("CARGO_TARGET_TMPDIR"))
}

use std::collections::BTreeMap;
use std::io;

use slip_model::edits::{KeyId, Piece, Place, tiers};
use slip_model::editset::{Edit, EditSet, SCHEME, SetState};
use slip_model::formats;
use slip_model::hash::content_digest;
use slip_model::{Digest, FileRef};

use slip_apply::Files;

/// A pre-image in memory, by path.
#[derive(Clone, Debug, Default)]
pub struct Mem(pub BTreeMap<Vec<u8>, Vec<u8>>);

impl Files for Mem {
    fn read(&self, path: &[u8]) -> io::Result<Vec<u8>> {
        self.0.get(path).cloned().ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

pub fn lit(b: &[u8]) -> Piece {
    Piece::Literal(b.to_vec())
}

pub fn spell(key: u8, b: &[u8]) -> Piece {
    Piece::Spelled { key: key_id(key), text: b.to_vec() }
}

pub fn copy(file: u64, lo: u64, hi: u64) -> Piece {
    Piece::Copy(Place { file, lo, hi })
}

pub fn key_id(k: u8) -> KeyId {
    Digest([k; 32])
}

pub fn edit(file: u64, lo: u64, hi: u64, pieces: Vec<Piece>) -> Edit {
    Edit {
        file,
        lo,
        hi,
        pieces,
        subjects: vec![],
        tiers: tiers::T0,
        macro_site: None,
        notes: vec![],
        observers: 1,
        votes: 1,
        first: vec![],
    }
}

/// An accepted edit set over `files` (path, bytes), sorted by path, with
/// these edits (sorted here), its id and the pre-image.
pub fn set(files: &[(&str, &[u8])], mut edits: Vec<Edit>) -> (EditSet, Digest, Mem) {
    let mut files: Vec<(&str, &[u8])> = files.to_vec();
    files.sort();
    edits.sort_by_key(|e| (e.file, e.lo, e.hi));
    let refs = files
        .iter()
        .map(|(p, b)| FileRef {
            path: p.as_bytes().to_vec(),
            size: b.len() as u64,
            content: content_digest(b),
        })
        .collect();
    let set = EditSet {
        scheme: SCHEME,
        plan: Digest([1; 32]),
        round: 1,
        transform: Digest([2; 32]),
        basis: Digest([3; 32]),
        state: SetState::Accepted,
        files: refs,
        subjects: vec![],
        rules: vec![],
        edits,
        groups: vec![],
        changed: vec![],
        kept: vec![],
        unresolved: vec![],
        refusals: vec![],
        reports: vec![],
        failures: vec![],
        unparsed: vec![],
        counters: vec![],
        instances: Vec::new(),
    };
    let mem = Mem(files.iter().map(|(p, b)| (p.as_bytes().to_vec(), b.to_vec())).collect());
    let id = id_of(&set);
    (set, id, mem)
}

/// The content id of a valid edit set.
pub fn id_of(set: &EditSet) -> Digest {
    set.validate().unwrap_or_else(|e| panic!("test edit set is invalid: {e}"));
    formats::content_id(&set.to_bytes()).expect("a valid set encodes")
}

/// A small deterministic generator (xorshift64*).
#[derive(Clone, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn chance(&mut self, num: u64, den: u64) -> bool {
        self.below(den) < num
    }

    /// `min + below(extra)` random bytes drawn from `alphabet`.
    pub fn bytes_n(&mut self, min: usize, extra: u64, alphabet: &[u8]) -> Vec<u8> {
        let len = min + self.below(extra) as usize;
        self.bytes(len, alphabet)
    }

    /// Random bytes drawn from `alphabet`.
    pub fn bytes(&mut self, len: usize, alphabet: &[u8]) -> Vec<u8> {
        (0..len).map(|_| alphabet[self.below(alphabet.len() as u64) as usize]).collect()
    }
}

/// Removes a scratch tree. The workspace's file system, under load, has
/// answered "directory not empty" for a directory just emptied and "not
/// found" for an entry it had just listed; neither is the test's
/// subject, so the removal is retried for a second before it fails.
pub fn remove_tree(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    let mut tries = 0;
    loop {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(e) if e.kind() == io::ErrorKind::NotFound && !path.exists() => return,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::NotFound
                ) && tries < 20 =>
            {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => panic!("{}: cannot remove: {e}", path.display()),
        }
    }
}
