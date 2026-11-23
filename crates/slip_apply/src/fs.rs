//! [`DiskTree`]: [`Tree`] over the real file system (`spec/edits.md`
//! §7.3), through `std::fs` alone.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::journal::parent;
use crate::write::{Stat, Tree};

/// The working tree on disk. Paths are absolute real paths, as bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiskTree;

fn path(p: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(p))
}

impl Tree for DiskTree {
    /// `symlink_metadata` of the path. `regular` is false for anything but
    /// a regular file, and also when the path's directory is not its own
    /// real path (a directory on the path is a symbolic link), so that the
    /// file replaced is always the one at the path as written.
    /// `dir_writable` is the directory's permission bits granting a write
    /// to someone (`!readonly()`): std has no `access(2)`, so a directory
    /// that grants it to another user only is found at the create of
    /// step 3, which fails and is undone.
    fn stat(&self, p: &[u8]) -> io::Result<Option<Stat>> {
        let m = match fs::symlink_metadata(path(p)) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let dir = parent(p);
        let real_dir = fs::canonicalize(path(dir))?;
        let regular = m.file_type().is_file() && real_dir.as_os_str().as_bytes() == dir;
        let dir_writable = !fs::metadata(path(dir))?.permissions().readonly();
        Ok(Some(Stat { regular, links: m.nlink(), mode: m.mode() & 0o7777, dir_writable }))
    }

    fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
        fs::read(path(p))
    }

    /// Opened with `create_new` when `exclusive`, written, given exactly
    /// `mode` (not masked by the umask), then `sync_all`.
    fn create(&self, p: &[u8], mode: u32, bytes: &[u8], exclusive: bool) -> io::Result<()> {
        let mut o = OpenOptions::new();
        o.write(true).mode(mode & 0o7777);
        if exclusive {
            o.create_new(true);
        } else {
            o.create(true).truncate(true);
        }
        let mut f = o.open(path(p))?;
        f.write_all(bytes)?;
        f.set_permissions(Permissions::from_mode(mode & 0o7777))?;
        f.sync_all()
    }

    fn rename(&self, from: &[u8], to: &[u8]) -> io::Result<()> {
        fs::rename(path(from), path(to))
    }

    fn remove(&self, p: &[u8]) -> io::Result<()> {
        fs::remove_file(path(p))
    }

    /// Opens the directory and syncs it.
    fn sync_dir(&self, dir: &[u8]) -> io::Result<()> {
        File::open(path(dir))?.sync_all()
    }
}
