//! Negative inputs (work package WP-117): the places the map parse looked
//! for a file and found none, as the map shard records them
//! (`spec/model.md` §8.6), and guard N1 of `spec/verify.md` §5.3, which
//! refuses a verdict when a file now exists at one of them.

use std::ffi::OsStr;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

use slip_kiln::Lookups;

use crate::image::Resolver;

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
