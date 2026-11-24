//! The scan step (`spec/edits.md` §8.4): the text that spells a rename's
//! old name and that no observer accounted for. Not part of the trusted
//! base: it changes no verdict, it says where verification did not look.
//!
//! Files are raw-lexed by Kiln (`slip_kiln::scan`); nothing here reads C++
//! text itself. Each file is read once, the files in parallel on at most
//! the threads given; the result is sorted, so it does not depend on them.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use slip_model::editset::EditSet;
use slip_model::hash::content_digest;
use slip_model::result::{Category, Residue};
use slip_model::transform::{Ownership, Transform};

/// §8.4's extensions, without `--residue-ext`.
pub const EXTENSIONS: [&str; 14] = [
    ".h", ".hh", ".hpp", ".hxx", ".h++", ".c", ".cc", ".cpp", ".cxx", ".c++", ".ipp", ".tcc",
    ".inc", ".def",
];

/// `--residue-ext`'s entries, or §8.4's: each with a leading `.`.
pub fn extensions(given: Option<&[Vec<u8>]>) -> Vec<Vec<u8>> {
    match given {
        None => EXTENSIONS.iter().map(|e| e.as_bytes().to_vec()).collect(),
        Some(list) => list
            .iter()
            .map(|e| if e.first() == Some(&b'.') { e.clone() } else { [b".", &e[..]].concat() })
            .collect(),
    }
}

/// Ranges of one file, for "is this range inside one of them" in
/// logarithmic time: sorted by `lo`, with the greatest `hi` so far.
struct Cover {
    lo: Vec<u64>,
    reach: Vec<u64>,
}

impl Cover {
    fn new(mut ranges: Vec<(u64, u64)>) -> Cover {
        ranges.sort_unstable();
        let mut reach = Vec::with_capacity(ranges.len());
        let mut r = 0;
        for &(_, hi) in &ranges {
            r = r.max(hi);
            reach.push(r);
        }
        Cover { lo: ranges.into_iter().map(|(lo, _)| lo).collect(), reach }
    }

    /// Some range `[a, b)` has `a <= lo` and `hi <= b`.
    fn covers(&self, lo: u64, hi: u64) -> bool {
        let n = self.lo.partition_point(|&a| a <= lo);
        n > 0 && self.reach[n - 1] >= hi
    }
}

/// What covers the hits of one file of the set.
struct Covers {
    edits: Cover,
    kept: Cover,
    unresolved: Cover,
    refusals: Cover,
}

fn by_file(places: impl Iterator<Item = (u64, u64, u64)>) -> HashMap<u64, Vec<(u64, u64)>> {
    let mut m: HashMap<u64, Vec<(u64, u64)>> = HashMap::new();
    for (f, lo, hi) in places {
        m.entry(f).or_default().push((lo, hi));
    }
    m
}

/// Whether `path` ends in one of `extensions`.
fn has_extension(path: &[u8], extensions: &[Vec<u8>]) -> bool {
    extensions.iter().any(|e| path.len() > e.len() && path.ends_with(e))
}

/// The regular files under owned entry `entry` (§8.4), owned and with one
/// of `extensions`: directories in byte order of names, no symbolic link
/// followed. An owned entry that is itself such a file is one.
fn walk(
    entry: &[u8],
    ownership: &Ownership,
    extensions: &[Vec<u8>],
    found: &mut BTreeSet<Vec<u8>>,
) -> io::Result<()> {
    let path = Path::new(std::ffi::OsStr::from_bytes(entry));
    let with_path = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
    let meta = fs::symlink_metadata(path).map_err(with_path)?;
    if meta.is_file() {
        if has_extension(entry, extensions) && ownership.is_owned(entry) {
            found.insert(entry.to_vec());
        }
        return Ok(());
    }
    if !meta.is_dir() {
        return Ok(());
    }
    let mut names: Vec<Vec<u8>> = fs::read_dir(path)
        .map_err(with_path)?
        .map(|d| d.map(|d| d.file_name().as_bytes().to_vec()))
        .collect::<io::Result<_>>()
        .map_err(with_path)?;
    names.sort();
    for name in names {
        let mut child = entry.to_vec();
        if child.last() != Some(&b'/') {
            child.push(b'/');
        }
        child.extend_from_slice(&name);
        walk(&child, ownership, extensions, found)?;
    }
    Ok(())
}

/// The files to scan, each file once (WP-076 fix round, should-fix 5):
/// paths that are one file by identity (device and inode, links
/// followed: a hard link, two spellings on a case-insensitive volume)
/// are scanned once, under the path the edit set names, else one some
/// observer read, else the least; with whether some observer read it
/// under any of them. A path that cannot be looked up is its own file
/// (reading it fails and names it).
fn by_identity(
    files: BTreeSet<Vec<u8>>,
    set: &EditSet,
    read_by_some: &[Vec<u8>],
) -> Vec<(Vec<u8>, bool)> {
    use std::os::unix::fs::MetadataExt as _;
    let read = |p: &[u8]| read_by_some.binary_search_by(|r| r.as_slice().cmp(p)).is_ok();
    let rank = |p: &[u8]| (set.file_index(p).is_none(), !read(p));
    let mut groups: HashMap<(u64, u64), usize> = HashMap::new();
    let mut out: Vec<(Vec<u8>, bool)> = Vec::with_capacity(files.len());
    for path in files {
        let id = fs::metadata(std::ffi::OsStr::from_bytes(&path)).ok().map(|m| (m.dev(), m.ino()));
        let was_read = read(&path);
        match id.and_then(|id| groups.get(&id).copied()) {
            Some(k) => {
                // Files come in byte order: the least path is already there.
                out[k].1 |= was_read;
                if rank(&path) < rank(&out[k].0) {
                    out[k].0 = path;
                }
            }
            None => {
                if let Some(id) = id {
                    groups.insert(id, out.len());
                }
                out.push((path, was_read));
            }
        }
    }
    out.sort();
    out
}

/// §8.4: the residue of a rename request. `read_by_some` holds, sorted,
/// the owned real paths in the `inputs` of some `ok` map shard of the
/// round; `unparsed_mains` the main files of the unparsed observers under
/// `--allow-unparsed` (scanned as files no observer read; empty without
/// it). A script request has no residue. An error names the file.
pub fn residue(
    transform: &Transform,
    set: &EditSet,
    read_by_some: &[Vec<u8>],
    unparsed_mains: &[Vec<u8>],
    extensions: &[Vec<u8>],
    threads: usize,
) -> io::Result<Vec<Residue>> {
    let Some(name) = transform.rename.as_ref().and_then(|r| r.target.last()) else {
        return Ok(Vec::new());
    };
    let ownership = &transform.ownership;
    let mut files: BTreeSet<Vec<u8>> = read_by_some.iter().cloned().collect();
    files.extend(unparsed_mains.iter().filter(|m| ownership.is_owned(m)).cloned());
    for entry in &ownership.owned {
        walk(entry, ownership, extensions, &mut files)?;
    }
    let files = by_identity(files, set, read_by_some);

    let mut edits = by_file(set.edits.iter().map(|e| (e.file, e.lo, e.hi)));
    let mut kept = by_file(set.kept.iter().map(|p| (p.file, p.lo, p.hi)));
    let mut unresolved = by_file(set.unresolved.iter().map(|p| (p.file, p.lo, p.hi)));
    let mut refusals = by_file(set.refusals.iter().filter_map(|r| Some((r.file?, r.lo, r.hi))));
    let covers: HashMap<u64, Covers> = (0..set.files.len() as u64)
        .map(|f| {
            let take = |m: &mut HashMap<u64, Vec<(u64, u64)>>| {
                Cover::new(m.remove(&f).unwrap_or_default())
            };
            let c = Covers {
                edits: take(&mut edits),
                kept: take(&mut kept),
                unresolved: take(&mut unresolved),
                refusals: take(&mut refusals),
            };
            (f, c)
        })
        .collect();

    let scan_one = |i: usize| -> io::Result<Vec<Residue>> {
        let (path, read) = (&files[i].0, files[i].1);
        let shown = String::from_utf8_lossy(path);
        let bytes = fs::read(std::ffi::OsStr::from_bytes(path))
            .map_err(|e| io::Error::new(e.kind(), format!("{shown}: {e}")))?;
        let cover = match set.file_index(path) {
            Some(f) => {
                let r = &set.files[f as usize];
                if r.size != bytes.len() as u64 || r.content != content_digest(&bytes) {
                    return Err(io::Error::other(format!(
                        "{shown}: the file is not the one the edit set names (it changed after \
                         the map phase)"
                    )));
                }
                covers.get(&f)
            }
            None => None,
        };
        let mut out = Vec::new();
        for h in slip_kiln::scan::scan(&bytes, name) {
            let at = |c: fn(&Covers) -> &Cover| cover.is_some_and(|x| c(x).covers(h.lo, h.hi));
            let category = if at(|c| &c.edits) {
                continue;
            } else if h.mention {
                Category::Mention
            } else if at(|c| &c.kept) || at(|c| &c.refusals) {
                continue;
            } else if at(|c| &c.unresolved) {
                Category::Dependent
            } else if read {
                Category::NotCompiled
            } else {
                Category::NotPlanned
            };
            out.push(Residue {
                category,
                file: path.clone(),
                lo: h.lo,
                hi: h.hi,
                line: h.line,
                column: h.column,
            });
        }
        Ok(out)
    };
    let mut found = in_file_order(files.len(), threads, scan_one)?;
    found.sort_by(|a, b| (&a.file, a.lo).cmp(&(&b.file, b.lo)));
    found.dedup_by(|a, b| a.file == b.file && a.lo == b.lo);
    Ok(found)
}

/// `scan(i)` for every `i < n` on at most `threads` threads, the results
/// concatenated; the error of the least `i` that failed, so that which is
/// reported does not depend on the threads.
fn in_file_order<T: Send>(
    n: usize,
    threads: usize,
    scan: impl Fn(usize) -> io::Result<Vec<T>> + Sync,
) -> io::Result<Vec<T>> {
    let next = AtomicUsize::new(0);
    let found: Mutex<Vec<T>> = Mutex::new(Vec::new());
    let failed: Mutex<Option<(usize, io::Error)>> = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..threads.clamp(1, n.max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    match scan(i) {
                        Ok(mut v) => {
                            found.lock().unwrap_or_else(PoisonError::into_inner).append(&mut v);
                        }
                        Err(e) => {
                            let mut f = failed.lock().unwrap_or_else(PoisonError::into_inner);
                            if f.as_ref().is_none_or(|(j, _)| i < *j) {
                                *f = Some((i, e));
                            }
                        }
                    }
                }
            });
        }
    });
    if let Some((_, e)) = failed.into_inner().unwrap_or_else(PoisonError::into_inner) {
        return Err(e);
    }
    Ok(found.into_inner().unwrap_or_else(PoisonError::into_inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_by_containment() {
        let c = Cover::new(vec![(10, 20), (0, 3), (12, 14)]);
        assert!(c.covers(0, 3));
        assert!(c.covers(12, 14));
        assert!(c.covers(15, 20));
        assert!(!c.covers(2, 4));
        assert!(!c.covers(20, 21));
        assert!(!Cover::new(Vec::new()).covers(0, 1));
    }

    #[test]
    fn extensions_given_and_default() {
        assert_eq!(extensions(None).len(), 14);
        assert_eq!(
            extensions(Some(&[b"inl".to_vec(), b".x".to_vec()])),
            [b".inl".to_vec(), b".x".to_vec()]
        );
        assert!(has_extension(b"/a/b.h", &extensions(None)));
        assert!(!has_extension(b"/a/b.hpp~", &extensions(None)));
    }
}
