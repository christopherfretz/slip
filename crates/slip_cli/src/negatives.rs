//! Negative inputs (work package WP-117): the places the map parse looked
//! for a file and found none, as the map shard records them
//! (`spec/model.md` §8.6), and guard N1 of `spec/verify.md` §5.3, which
//! refuses a verdict when a file now exists at one of them. Their positive
//! counterpart (work package WP-135): the places where it found one, with
//! the input each led to, and the inputs that were one file
//! (`spec/model.md` §8.8), and guard N2, which refuses a verdict when
//! either differs now.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

use slip_kiln::Lookups;
use slip_model::{FileRef, Found, FoundPlace};

use crate::image::{Resolver, show_path};

/// The negative inputs of a parse run from `directory` (the command's) that
/// looked at `lookups`: the path of each place of `Lookups::missed`, unique
/// and sorted by bytes (`spec/model.md` §8.6, "The path of a place").
///
/// A place `(d, n)` is the path `directory/d/n` (`d` or `n` absolute
/// replaces what is before it) as written, with its `.` components and
/// repeated `/` dropped: nothing is resolved, no symbolic link followed and
/// no `..` taken, so the path names what the kernel would reach from it
/// when the guard looks, wherever its links then lead.
pub fn places(directory: &[u8], lookups: &Lookups) -> Vec<Vec<u8>> {
    let base = Path::new(OsStr::from_bytes(directory));
    let mut out: Vec<Vec<u8>> =
        lookups.missed.iter().map(|(d, n)| place(&base.join(d).join(n))).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// `path` with its `.` components and repeated `/` dropped (`..` kept).
fn place(path: &Path) -> Vec<u8> {
    let mut out = PathBuf::from("/");
    for c in path.components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::ParentDir => out.push(".."),
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    out.into_os_string().into_vec()
}

/// Whether a file exists at `path` as Kiln's lookup decides it (Kiln
/// `SourceManager::load_regular_file`, `RealFs::read`): the path, its
/// symbolic links followed now, names a regular file that can be opened.
/// A directory, a dangling link, a device or pipe, and a file that cannot
/// be opened are not one. (Kiln also gives up on a file it opens and
/// cannot read to its end; such a file exists here: the guard refuses more,
/// never less.)
pub fn exists(path: &[u8]) -> bool {
    let path = Path::new(OsStr::from_bytes(path));
    std::fs::metadata(path).is_ok_and(|m| m.is_file()) && std::fs::File::open(path).is_ok()
}

/// Guard N1 (`spec/verify.md` §5.3): the first negative input of the pre
/// shard, in path order, at which a file now exists ([`exists`]) whose
/// real path (links and `..` resolved now) is not under a subject root
/// ([`Resolver::under_subject`], by identity). The real path, not the
/// place as written: a place spelled through a subject root
/// (`s/../ext/h.h`) can name a file outside it. A file whose real path
/// cannot be had is taken as outside: refused.
pub fn appeared<'a>(resolver: &mut Resolver<'_>, negatives: &'a [Vec<u8>]) -> Option<&'a [u8]> {
    negatives
        .iter()
        .find(|p| {
            exists(p)
                && !std::fs::canonicalize(Path::new(OsStr::from_bytes(p)))
                    .is_ok_and(|real| resolver.under_subject(real.as_os_str().as_bytes()))
        })
        .map(Vec::as_slice)
}

// ---------------------------------------------------------------------------
// Found lookups (work package WP-135): `spec/model.md` §8.8, guard N2 of
// `spec/verify.md` §5.3.

/// The identity of the file `path` leads to now (links followed): its
/// device and inode, read and compared here only, never recorded.
fn identity(path: &[u8]) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(Path::new(OsStr::from_bytes(path))).ok().map(|m| (m.dev(), m.ino()))
}

/// The found places of a parse run from `directory` that looked at
/// `lookups` (`spec/model.md` §8.8): the path of each place of
/// `Lookups::found` and of each name of `Lookups::read` (the main file and
/// every buffer as the front end first spelled it, so a file reached
/// through a spelling the log does not record is covered as well), each
/// made a path as [`places`] makes one, unique and sorted by bytes.
pub fn found_places(directory: &[u8], lookups: &Lookups) -> Vec<Vec<u8>> {
    let base = Path::new(OsStr::from_bytes(directory));
    let mut out: Vec<Vec<u8>> = lookups
        .found
        .iter()
        .map(|(d, n)| place(&base.join(d).join(n)))
        .chain(lookups.read.iter().map(|n| place(&base.join(n))))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The `found` section of a map shard (`spec/model.md` §8.8) whose parse
/// found `places` ([`found_places`]) and read `inputs`, made right after
/// the parse: each place with the input it leads to (the input whose path
/// is the place's real path, else the input that is the same file, by
/// identity, as the place: a hard link the front end read as one buffer),
/// and the classes of inputs that are one file (by identity; each of two
/// inputs or more, ascending).
///
/// # Errors
///
/// An input that cannot be read, or a place that leads to no input: the
/// tree changed during the parse (the assumption the shard's `inputs`
/// already make, `spec/model.md` §8.4); the map task fails rather than
/// record a place it cannot tie to what was read.
pub fn record(places: Vec<Vec<u8>>, inputs: &[FileRef]) -> Result<Found, String> {
    let ids: Vec<(u64, u64)> = inputs
        .iter()
        .map(|f| {
            identity(&f.path).ok_or_else(|| {
                format!("{} changed during the parse: it cannot be read", show_path(&f.path))
            })
        })
        .collect::<Result<_, _>>()?;
    let mut by_id: HashMap<(u64, u64), Vec<u64>> = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        by_id.entry(*id).or_default().push(i as u64);
    }
    let mut out = Found::default();
    for path in places {
        let real = std::fs::canonicalize(Path::new(OsStr::from_bytes(&path)))
            .ok()
            .map(PathBuf::into_os_string);
        let by_path = real
            .and_then(|r| inputs.binary_search_by(|f| f.path.as_slice().cmp(r.as_bytes())).ok());
        let input = match by_path {
            Some(i) => i as u64,
            None => match identity(&path).and_then(|id| by_id.get(&id)) {
                Some(class) => class[0],
                None => {
                    return Err(format!(
                        "{} changed during the parse: the place leads to none of its inputs",
                        show_path(&path)
                    ));
                }
            },
        };
        out.places.push(FoundPlace { path, input });
    }
    out.classes = by_id.into_values().filter(|c| c.len() > 1).collect();
    out.classes.sort_unstable();
    Ok(out)
}

/// The walk of `path` now: its real path when it exists, else the real
/// path of its deepest prefix that exists followed by the rest as written
/// (the place a file would be at, with every link on the way resolved).
fn walk(path: &[u8]) -> Vec<u8> {
    let path = Path::new(OsStr::from_bytes(path));
    if let Ok(real) = std::fs::canonicalize(path) {
        return real.into_os_string().into_vec();
    }
    let parts: Vec<Component<'_>> = path.components().collect();
    for k in (1..parts.len()).rev() {
        let prefix: PathBuf = parts[..k].iter().collect();
        if let Ok(mut real) = std::fs::canonicalize(&prefix) {
            for c in &parts[k..] {
                real.push(c);
            }
            return real.into_os_string().into_vec();
        }
    }
    path.as_os_str().as_bytes().to_vec()
}

/// Guard N2 (`spec/verify.md` §5.3): the reason when a found lookup of the
/// map parse would now lead elsewhere, or when the inputs that were one
/// file at the map phase are not one file now (or the other way round);
/// `None` when neither.
///
/// (i) For each found place in path order, under a subject root or not:
/// its walk now ([`walk`]) is the path of the input it led to, or it is the
/// same file as that input by identity (a hard link the front end read as
/// one buffer). The place as written is walked now, links and all: a
/// retargeted link is seen wherever it is. The file's existence and
/// content are not judged here (a subject file is the post tree's to give;
/// an outside one O2's).
///
/// (ii) Two inputs are one file now exactly when they were one class at
/// the map phase, every pair, compared twice, each time over the pairs
/// both of whose members can be read there: at the recorded paths (would a
/// map parse now read them as the map parse did? `#pragma once` and the
/// buffers follow identity, which no content check sees), and where the
/// post parse reads them, an input of the subject (as `left_behind`
/// decides it) at its post tree's counterpart (is the partition the post
/// parse sees the recorded one? The post tree as verified must be the post
/// tree as delivered, and a delivery that copies bytes makes two files of
/// one again).
pub fn found_moved(
    resolver: &mut Resolver<'_>,
    found: &Found,
    inputs: &[FileRef],
) -> Option<String> {
    for p in &found.places {
        let Some(to) = usize::try_from(p.input).ok().and_then(|i| inputs.get(i)) else {
            return Some(format!(
                "pre-image: the map shard's found place {} names no input (run `slip parse` again)",
                show_path(&p.path)
            ));
        };
        let now = walk(&p.path);
        if now == to.path {
            continue;
        }
        if identity(&p.path).is_some_and(|id| identity(&to.path) == Some(id)) {
            continue;
        }
        return Some(format!(
            "pre-image: {} now leads to {}, not to {} (run `slip parse` again)",
            show_path(&p.path),
            show_path(&now),
            show_path(&to.path)
        ));
    }
    let mut class_of: Vec<Option<usize>> = vec![None; inputs.len()];
    for (j, c) in found.classes.iter().enumerate() {
        for &x in c {
            if let Some(slot) = usize::try_from(x).ok().and_then(|x| class_of.get_mut(x)) {
                *slot = Some(j);
            }
        }
    }
    // Each input's identity at its recorded path (the subject as the map
    // phase read it, the outside now) and where the post parse reads it
    // (a subject input at its post counterpart; an outside input is the
    // same file in both).
    let mut at_pre = Vec::with_capacity(inputs.len());
    let mut at_post = Vec::with_capacity(inputs.len());
    for f in inputs {
        let out = !(crate::verify_job::as_recorded(&f.path) && resolver.under_subject(&f.path));
        let id = identity(&f.path);
        at_pre.push(id);
        at_post.push(if out {
            id
        } else {
            resolver.post_counterpart(&f.path).and_then(|at| identity(&at))
        });
    }
    let mut first: Option<(usize, usize, bool)> = None;
    let mut consider = |a: usize, b: usize, merged: bool| {
        let (a, b) = (a.min(b), a.max(b));
        if first.is_none_or(|(x, y, _)| (a, b) < (x, y)) {
            first = Some((a, b, merged));
        }
    };
    // Every pair, in both: is the map shard what a map parse would give
    // now? And is the post tree as verified the post tree as delivered (a
    // delivery that copies bytes makes two files of one again)?
    for ids in [&at_pre, &at_post] {
        // One class then, two files now.
        for c in &found.classes {
            let c: Vec<usize> = c.iter().filter_map(|&x| usize::try_from(x).ok()).collect();
            for (k, &a) in c.iter().enumerate() {
                for &b in &c[k + 1..] {
                    let (Some(Some(ia)), Some(Some(ib))) = (ids.get(a), ids.get(b)) else {
                        continue;
                    };
                    if ia != ib {
                        consider(a, b, false);
                    }
                }
            }
        }
        // One file now, two then.
        let mut now: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            if let Some(id) = id {
                now.entry(*id).or_default().push(i);
            }
        }
        for group in now.values() {
            for (k, &a) in group.iter().enumerate() {
                for &b in &group[k + 1..] {
                    if class_of[a].is_none() || class_of[a] != class_of[b] {
                        consider(a, b, true);
                    }
                }
            }
        }
    }
    first.map(|(a, b, merged)| {
        let (a, b) = (show_path(&inputs[a].path), show_path(&inputs[b].path));
        if merged {
            format!(
                "pre-image: {a} and {b} are one file now, two at the map phase (run `slip parse` again)"
            )
        } else {
            format!(
                "pre-image: {a} and {b} were one file at the map phase, two now (run `slip parse` again)"
            )
        }
    })
}
