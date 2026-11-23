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
        Ok(Some(Stat {
            regular,
            links: m.nlink(),
            mode: m.mode() & 0o7777,
            dir_writable,
            uid: m.uid(),
            gid: m.gid(),
            dev: m.dev(),
            ino: m.ino(),
            size: m.size(),
            mtime: (m.mtime(), m.mtime_nsec()),
            ctime: (m.ctime(), m.ctime_nsec()),
        }))
    }

    fn read(&self, p: &[u8]) -> io::Result<Vec<u8>> {
        fs::read(path(p))
    }

    /// Opened with `create_new` (nothing at the name is followed or
    /// truncated), written and synced; then opened again for reading, and
    /// that handle checked to be the same file (device and inode) as the
    /// one written, so that a name swapped for a link in between is found
    /// and its target untouched; given exactly `mode` through the checked
    /// handle (not masked by the umask), synced, and the bits read back: a
    /// file system that does not keep them fails the create. The bits are
    /// set through a handle not open for writing because some file systems
    /// clear setuid and setgid on a change made through a writable one.
    fn create(&self, p: &[u8], mode: u32, bytes: &[u8]) -> io::Result<()> {
        let mode = mode & 0o7777;
        let mut f =
            OpenOptions::new().write(true).create_new(true).mode(mode & 0o777).open(path(p))?;
        f.write_all(bytes)?;
        f.sync_all()?;
        let written = f.metadata()?;
        // The write handle stays open across the reopen: some file systems
        // (FUSE-backed host mounts) number a file anew once nothing holds
        // it, which would read as a swap.
        let reopened_file = File::open(path(p))?;
        let reopened = reopened_file.metadata()?;
        drop(f);
        let f = reopened_file;
        if (reopened.dev(), reopened.ino()) != (written.dev(), written.ino()) {
            return Err(io::Error::other(
                "the staged name was replaced while it was being created",
            ));
        }
        f.set_permissions(Permissions::from_mode(mode))?;
        f.sync_all()?;
        let kept = f.metadata()?.mode() & 0o7777;
        if kept != mode {
            return Err(io::Error::other(format!(
                "the file system kept permission bits {kept:o} of {mode:o}"
            )));
        }
        Ok(())
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
