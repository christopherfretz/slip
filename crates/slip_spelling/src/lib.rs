//! One spelling per file (`spec/edits.md` §2.1, "One spelling";
//! `spec/landed/edits/WP-147.md`).
//!
//! On a volume that finds a name in any case, or in any Unicode
//! normalisation, and keeps the spelling it was asked with,
//! `std::fs::canonicalize` returns the spelling it was **asked**: one file
//! has as many real paths as its inputs have spellings of it. Slip gives
//! every path, where it enters, the spelling its directories list
//! ([`real_path`], [`Spelling::on_disk`]), and `slip_apply` checks that a
//! path it is handed already has it.
//!
//! The walk decides, per directory (never per mount: ext4, f2fs and tmpfs
//! fold per directory), whether the directory's names must be listed:
//!
//! - A directory is `NotFolding` when a component of it whose every byte
//!   is ASCII and which has an ASCII letter finds, with every ASCII
//!   letter's case swapped, nothing or another file (`lstat`, device and
//!   inode), and its device is one whose lookups are answered
//!   consistently ([`Lookup::consistent`]). Only there does a component
//!   pass without a listing, and only an all-ASCII one.
//! - Everywhere else the directory is listed, once, and a component passes
//!   only when it is, byte for byte, a listed name. A component that is
//!   not is resolved by identity: `lstat` of it, compared with each listed
//!   name that could be it. A lookup's "nothing there" is never taken as a
//!   pass: it is [`SpellingError::Missing`].
//! - A component with a non-ASCII byte always lists its directory and
//!   never decides its state (ext4's casefolding compares a name that is
//!   not UTF-8 by its bytes, so such a probe can miss in a directory that
//!   folds every other name); so does one with no ASCII letter in a
//!   directory not yet decided (a casefolding directory finds a stored
//!   `1\u{200B}3` as `13`).
//!
//! The cache keys each directory's state by its identity (device and
//! inode) and each walked prefix by its spelling on disk, never by the
//! spelling asked.

#![forbid(unsafe_code)]

mod disk;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

pub use disk::{CONSISTENT, Disk, Mounts, makedev};

/// Which file: a device and an inode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id {
    pub dev: u64,
    pub ino: u64,
}

/// What the walk asks of a file system: the disk ([`Disk`]), or a test's
/// tree.
pub trait Lookup {
    /// `lstat` (a link is not followed): `None` when nothing is there
    /// (`NotFound`, and `NotADirectory` for a path through a file).
    fn lstat(&self, path: &[u8]) -> io::Result<Option<Id>>;
    /// `read_dir`'s names, byte for byte as the file system keeps them;
    /// `None` when `dir` is missing or is not a directory.
    fn names(&self, dir: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>>;
    /// Whether lookups on device `dev` are answered consistently, so that
    /// a lookup finding nothing in another case shows that the directory
    /// does not fold (`spec/edits.md` §2.1; [`CONSISTENT`]).
    fn consistent(&self, dev: u64) -> bool;
}

/// Why a path has no spelling on disk.
#[derive(Debug)]
pub enum SpellingError {
    /// Not a canonical absolute path (relative, an empty, `.` or `..`
    /// component, a trailing `/`, a NUL byte).
    NotAPath(Vec<u8>),
    /// The path finds nothing: `at` (its prefix, spelled on disk up to its
    /// last component) is not there.
    Missing { path: Vec<u8>, at: Vec<u8> },
    /// A directory whose listing a component needs cannot be listed.
    Unlistable { path: Vec<u8>, dir: Vec<u8>, error: io::Error },
    /// The path finds a file and no listed name of its directory, or more
    /// than one, is that file. `names`: the ones that are.
    NoName { path: Vec<u8>, dir: Vec<u8>, names: Vec<Vec<u8>> },
    /// A lookup failed.
    Io { path: Vec<u8>, error: io::Error },
}

fn lossy(b: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(b)
}

impl SpellingError {
    /// The path asked.
    pub fn path(&self) -> &[u8] {
        match self {
            SpellingError::NotAPath(path)
            | SpellingError::Missing { path, .. }
            | SpellingError::Unlistable { path, .. }
            | SpellingError::NoName { path, .. }
            | SpellingError::Io { path, .. } => path,
        }
    }

    /// Nothing is there (the walk found nothing, or `canonicalize` said
    /// `NotFound`).
    pub fn is_not_found(&self) -> bool {
        match self {
            SpellingError::Missing { .. } => true,
            SpellingError::Io { error, .. } => error.kind() == io::ErrorKind::NotFound,
            _ => false,
        }
    }

    /// What went wrong, without the path asked (for a message that names
    /// it already).
    pub fn detail(&self) -> String {
        match self {
            SpellingError::NotAPath(_) => "not a canonical absolute path".to_owned(),
            SpellingError::Missing { at, .. } => {
                format!("no such file or directory ({})", lossy(at))
            }
            SpellingError::Unlistable { dir, error, .. } => format!(
                "the directory {} cannot be listed to find its spelling on disk: {error}",
                lossy(dir)
            ),
            SpellingError::NoName { dir, names, .. } => {
                let names: Vec<_> = names.iter().map(|n| lossy(n).into_owned()).collect();
                format!(
                    "no one name of the directory {} is this file ({})",
                    lossy(dir),
                    if names.is_empty() { "none".to_owned() } else { names.join(", ") }
                )
            }
            SpellingError::Io { error, .. } => error.to_string(),
        }
    }
}

impl fmt::Display for SpellingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", lossy(self.path()), self.detail())
    }
}

impl std::error::Error for SpellingError {}

/// A canonical absolute path (`slip_model::transform::is_canonical`): it
/// starts with `/`, has no empty, `.` or `..` component, no NUL byte, and
/// no trailing `/` unless it is `/` itself.
pub fn is_canonical(p: &[u8]) -> bool {
    if p.contains(&0) {
        return false;
    }
    match p.strip_prefix(b"/") {
        None => false,
        Some(b"") => true,
        Some(rest) => rest.split(|&b| b == b'/').all(|c| !matches!(c, b"" | b"." | b"..")),
    }
}

/// What the walk knows of one directory.
#[derive(Debug, Default)]
struct Dir {
    /// Seen not to fold (see the crate's documentation).
    not_folding: bool,
    /// Its listing, once read; or why it could not be.
    listing: Option<Result<Listing, (io::ErrorKind, String)>>,
}

#[derive(Debug)]
struct Listing {
    names: HashSet<Vec<u8>>,
}

#[derive(Debug, Default)]
struct Cache {
    /// Each walked prefix, by its spelling on disk: which file it is.
    prefixes: HashMap<Vec<u8>, Id>,
    /// Each directory, by its identity.
    dirs: HashMap<Id, Dir>,
}

/// The walk and its cache. Shared: `on_disk` takes `&self`, and holds the
/// cache's lock over one directory's step at a time (its lookups, its
/// probe or its listing), never over a whole path.
#[derive(Debug)]
pub struct Spelling<L> {
    lookup: L,
    cache: Mutex<Cache>,
}

fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(dir.len() + 1 + name.len());
    p.extend_from_slice(dir);
    if dir != b"/" {
        p.push(b'/');
    }
    p.extend_from_slice(name);
    p
}

/// `name` with the case of every ASCII letter swapped.
fn swap_case(name: &[u8]) -> Vec<u8> {
    name.iter()
        .map(|&b| {
            if b.is_ascii_lowercase() {
                b.to_ascii_uppercase()
            } else if b.is_ascii_uppercase() {
                b.to_ascii_lowercase()
            } else {
                b
            }
        })
        .collect()
}

/// A component that may decide its directory's state by a probe.
fn may_probe(name: &[u8]) -> bool {
    name.is_ascii() && name.iter().any(u8::is_ascii_alphabetic)
}

impl<L: Lookup> Spelling<L> {
    pub fn new(lookup: L) -> Spelling<L> {
        Spelling { lookup, cache: Mutex::new(Cache::default()) }
    }

    /// The lookup it walks.
    pub fn lookup(&self) -> &L {
        &self.lookup
    }

    /// The spelling on disk of `path`, a canonical absolute path whose
    /// links are resolved (`canonicalize`'s output): each component as its
    /// directory lists it. Links are not followed. Idempotent; the
    /// identity on a volume that does not fold.
    pub fn on_disk(&self, path: &[u8]) -> Result<Vec<u8>, SpellingError> {
        if !is_canonical(path) {
            return Err(SpellingError::NotAPath(path.to_vec()));
        }
        let mut out = b"/".to_vec();
        let mut dir = self.root(path)?;
        if path == b"/" {
            return Ok(out);
        }
        // Directories listed again during this walk: a name missing from
        // a listing re-lists it once (a file made since it was read).
        let mut relisted = HashSet::new();
        for name in path[1..].split(|&b| b == b'/') {
            let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            let asked = join(&out, name);
            if let Some(&id) = cache.prefixes.get(&asked) {
                out = asked;
                dir = id;
                continue;
            }
            let (spelled, id) = self.step(&mut cache, path, &out, dir, name, &mut relisted)?;
            let next = join(&out, &spelled);
            cache.prefixes.insert(next.clone(), id);
            out = next;
            dir = id;
        }
        Ok(out)
    }

    fn root(&self, path: &[u8]) -> Result<Id, SpellingError> {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(&id) = cache.prefixes.get(b"/".as_slice()) {
            return Ok(id);
        }
        let id = self.lstat(path, b"/")?;
        let Some(id) = id else {
            return Err(SpellingError::Missing { path: path.to_vec(), at: b"/".to_vec() });
        };
        cache.prefixes.insert(b"/".to_vec(), id);
        Ok(id)
    }

    fn lstat(&self, path: &[u8], p: &[u8]) -> Result<Option<Id>, SpellingError> {
        self.lookup.lstat(p).map_err(|error| SpellingError::Io { path: path.to_vec(), error })
    }

    /// The component `name` of `path` in the directory `dir_path` (spelled
    /// on disk), which is `dir`: its spelling on disk and its identity.
    fn step(
        &self,
        cache: &mut Cache,
        path: &[u8],
        dir_path: &[u8],
        dir: Id,
        name: &[u8],
        relisted: &mut HashSet<Id>,
    ) -> Result<(Vec<u8>, Id), SpellingError> {
        let at = join(dir_path, name);
        let missing = || SpellingError::Missing { path: path.to_vec(), at: at.clone() };
        let state = cache.dirs.entry(dir).or_default();
        if state.not_folding && name.is_ascii() {
            let id = self.lstat(path, &at)?.ok_or_else(missing)?;
            return Ok((name.to_vec(), id));
        }
        if state.listing.is_none() && may_probe(name) && self.lookup.consistent(dir.dev) {
            let id = self.lstat(path, &at)?.ok_or_else(missing)?;
            let other = self.lstat(path, &join(dir_path, &swap_case(name)))?;
            if other != Some(id) {
                state.not_folding = true;
                return Ok((name.to_vec(), id));
            }
        }
        // Listed: a listed name passes; another is found by identity.
        let names = self.listing(state, path, dir_path, false)?;
        if names.contains(name) {
            let id = self.lstat(path, &at)?.ok_or_else(missing)?;
            return Ok((name.to_vec(), id));
        }
        let id = self.lstat(path, &at)?.ok_or_else(missing)?;
        let mut found = self.by_identity(path, dir_path, names, name, id)?;
        if found.is_empty() && relisted.insert(dir) {
            let names = self.listing(state, path, dir_path, true)?;
            if names.contains(name) {
                return Ok((name.to_vec(), id));
            }
            found = self.by_identity(path, dir_path, names, name, id)?;
        }
        if found.len() == 1 {
            return Ok((found.swap_remove(0), id));
        }
        Err(SpellingError::NoName { path: path.to_vec(), dir: dir_path.to_vec(), names: found })
    }

    /// The directory's names, listed now if not yet (or `again`).
    fn listing<'d>(
        &self,
        state: &'d mut Dir,
        path: &[u8],
        dir_path: &[u8],
        again: bool,
    ) -> Result<&'d HashSet<Vec<u8>>, SpellingError> {
        if again || state.listing.is_none() {
            state.listing = Some(match self.lookup.names(dir_path) {
                Ok(Some(names)) => Ok(Listing { names: names.into_iter().collect() }),
                Ok(None) => {
                    state.listing = None;
                    return Err(SpellingError::Missing {
                        path: path.to_vec(),
                        at: dir_path.to_vec(),
                    });
                }
                Err(e) => Err((e.kind(), e.to_string())),
            });
        }
        match state.listing.as_ref() {
            Some(Ok(l)) => Ok(&l.names),
            Some(Err((kind, message))) => Err(SpellingError::Unlistable {
                path: path.to_vec(),
                dir: dir_path.to_vec(),
                error: io::Error::new(*kind, message.clone()),
            }),
            None => unreachable!("the listing was just read"),
        }
    }

    /// The listed names that are the file `id`, from the first of these
    /// that has any: the names equal to `name` but for ASCII case; the
    /// names with a non-ASCII byte, or equal to it under Unicode lower
    /// case (the Kelvin sign for `k`); every other name.
    fn by_identity(
        &self,
        path: &[u8],
        dir_path: &[u8],
        names: &HashSet<Vec<u8>>,
        name: &[u8],
        id: Id,
    ) -> Result<Vec<Vec<u8>>, SpellingError> {
        let lower = String::from_utf8_lossy(name).to_lowercase();
        let mut tiers: [Vec<&Vec<u8>>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for n in names {
            let tier = if n.eq_ignore_ascii_case(name) {
                0
            } else if !n.is_ascii() || String::from_utf8_lossy(n).to_lowercase() == lower {
                1
            } else {
                2
            };
            tiers[tier].push(n);
        }
        for mut tier in tiers {
            tier.sort();
            let mut found = Vec::new();
            for n in tier {
                if self.lstat(path, &join(dir_path, n))? == Some(id) {
                    found.push(n.clone());
                }
            }
            if !found.is_empty() {
                return Ok(found);
            }
        }
        Ok(Vec::new())
    }
}

/// The process-wide walk over the disk, shared by every thread.
pub fn disk() -> &'static Spelling<Disk> {
    static DISK: OnceLock<Spelling<Disk>> = OnceLock::new();
    DISK.get_or_init(|| Spelling::new(Disk::default()))
}

/// `std::fs::canonicalize`, then [`Spelling::on_disk`] through [`disk`]:
/// what every place a path enters Slip calls instead of `canonicalize`
/// (`spec/edits.md` §2.1). The cache's lock is not held over the
/// `canonicalize`, whose failure is `Io` with its own error.
pub fn real_path(path: &Path) -> Result<PathBuf, SpellingError> {
    let c = std::fs::canonicalize(path)
        .map_err(|error| SpellingError::Io { path: path.as_os_str().as_bytes().to_vec(), error })?;
    let spelled = disk().on_disk(c.as_os_str().as_bytes())?;
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(&spelled)))
}

/// `path` as given, respelled ([`Spelling::on_disk`] through [`disk`])
/// when it is its own real path (absolute, no `.` or `..`, no link on it:
/// `canonicalize` gives it back); otherwise, or when it cannot be
/// respelled, as given. For the records that keep a path as given (the
/// files a transform unit read, `spec/api.md` §2.2): the identity on a
/// volume that does not fold.
pub fn respelled(path: &[u8]) -> Vec<u8> {
    let p = Path::new(std::ffi::OsStr::from_bytes(path));
    match std::fs::canonicalize(p) {
        Ok(c) if c.as_os_str().as_bytes() == path => {
            disk().on_disk(path).unwrap_or_else(|_| path.to_vec())
        }
        _ => path.to_vec(),
    }
}
