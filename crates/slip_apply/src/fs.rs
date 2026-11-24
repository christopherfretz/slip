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
    /// truncated), written and synced. Then the name is checked to be still
    /// that file: `lstat`ed (a regular file of the written handle's device
    /// and inode, else the create fails before anything is opened through
    /// the name), opened again for reading, and that handle compared the
    /// same way, so that a name swapped for a link is found and its target
    /// never written or given bits. The open for reading by path still
    /// follows a swap that lands between the `lstat` and it (std has no
    /// `O_NOFOLLOW`): a FIFO there blocks the open, a device's open has its
    /// side effects; nothing is written or chmodded through it, and the
    /// journal is already published, so `recover` restores (§7.3, the
    /// recorded limit). Given exactly `mode` through the checked handle
    /// (not masked by the umask), synced, and the bits read back: a file
    /// system that does not keep them fails the create. The bits are set
    /// through a handle not open for writing because some file systems
    /// clear setuid and setgid on a change made through a writable one.
    fn create(&self, p: &[u8], mode: u32, bytes: &[u8]) -> io::Result<()> {
        create(p, mode, bytes, &|_| {})
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

    /// `read_dir`: `None` when `dir` is missing or is not a directory.
    fn names(&self, dir: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        let entries = match fs::read_dir(path(dir)) {
            Ok(e) => e,
            Err(e)
                if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let mut names = Vec::new();
        for entry in entries {
            names.push(entry?.file_name().as_bytes().to_vec());
        }
        Ok(Some(names))
    }
}

/// Where `create` lets a test change the file system (`seam`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    /// After the write and sync, before the `lstat` of the name.
    Written,
    /// After the `lstat`, before the name is opened again.
    Checked,
}

const SWAPPED: &str = "the staged name was replaced while it was being created";

/// `DiskTree::create`, with `seam` called at each [`Point`] (a no-op but in
/// this file's tests).
fn create(p: &[u8], mode: u32, bytes: &[u8], seam: &dyn Fn(Point)) -> io::Result<()> {
    let mode = mode & 0o7777;
    let mut f = OpenOptions::new().write(true).create_new(true).mode(mode & 0o777).open(path(p))?;
    f.write_all(bytes)?;
    f.sync_all()?;
    let written = f.metadata()?;
    let same = |m: &fs::Metadata| (m.dev(), m.ino()) == (written.dev(), written.ino());
    seam(Point::Written);
    // The write handle stays open until the checks are done: some file
    // systems (FUSE-backed host mounts) number a file anew once nothing
    // holds it, which would read as a swap.
    let named = fs::symlink_metadata(path(p))?;
    if !named.file_type().is_file() || !same(&named) {
        return Err(io::Error::other(SWAPPED));
    }
    seam(Point::Checked);
    let reopened = File::open(path(p))?;
    drop(f);
    if !same(&reopened.metadata()?) {
        return Err(io::Error::other(SWAPPED));
    }
    reopened.set_permissions(Permissions::from_mode(mode))?;
    reopened.sync_all()?;
    let kept = reopened.metadata()?.mode() & 0o7777;
    if kept != mode {
        return Err(io::Error::other(format!(
            "the file system kept permission bits {kept:o} of {mode:o}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Point, SWAPPED, create};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// A fresh directory under the target directory's `tmp` (what
    /// `CARGO_TARGET_TMPDIR` is for integration tests; unit tests are not
    /// given it): the test binary is `<target>/<profile>/deps/<name>`.
    fn root(name: &str) -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        let target = exe.parent().and_then(|d| d.parent()).and_then(|d| d.parent()).unwrap();
        let r = target.join("tmp").join(format!("slip-apply-fs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(&r).unwrap();
        r
    }

    fn mode(p: &std::path::Path) -> u32 {
        std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o7777
    }

    /// Swaps `name` for a symbolic link to `to` at `at`.
    fn swap(name: PathBuf, to: PathBuf, at: Point) -> impl Fn(Point) {
        move |p| {
            if p == at {
                let tmp = name.with_extension("lnk");
                std::os::unix::fs::symlink(&to, &tmp).unwrap();
                std::fs::rename(&tmp, &name).unwrap();
            }
        }
    }

    // §7.3 (`DiskTree::create`): the name swapped for a link to a victim
    // after the `lstat` and before the reopen: the reopened handle is
    // not the written file, the create fails, the victim keeps its bits.
    #[test]
    fn create_compares_the_reopened_handle() {
        let r = root("reopen");
        let victim = r.join("victim");
        std::fs::write(&victim, b"precious").unwrap();
        std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
        let staged = r.join(".f.slip-0000000000000000");
        let got = create(
            staged.as_os_str().as_bytes(),
            0o4755,
            b"post",
            &swap(staged.clone(), victim.clone(), Point::Checked),
        );
        assert_eq!(got.unwrap_err().to_string(), SWAPPED);
        assert_eq!(mode(&victim), 0o600);
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
        std::fs::remove_dir_all(&r).unwrap();
    }

    // §7.3 (`DiskTree::create`): the name swapped before the `lstat`, for
    // a link to a socket (a FIFO's stand-in: opening it fails at once
    // rather than blocking) and to a regular victim: the `lstat` finds it
    // and the create fails without opening the name again.
    #[test]
    fn create_lstats_the_name_before_the_reopen() {
        let r = root("lstat");
        let sock = r.join("sock");
        let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        let victim = r.join("victim");
        std::fs::write(&victim, b"precious").unwrap();
        std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
        for (i, to) in [sock.clone(), victim.clone()].into_iter().enumerate() {
            let staged = r.join(format!(".f{i}.slip-0000000000000000"));
            let got = create(
                staged.as_os_str().as_bytes(),
                0o4755,
                b"post",
                &swap(staged.clone(), to, Point::Written),
            );
            // Opening the socket would fail with its own error (ENXIO).
            assert_eq!(got.unwrap_err().to_string(), SWAPPED);
        }
        assert_eq!(mode(&victim), 0o600);
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
        std::fs::remove_dir_all(&r).unwrap();
    }

    // The seam left as a no-op: a create succeeds and keeps the bits.
    #[test]
    fn create_without_a_swap() {
        let r = root("plain");
        let staged = r.join(".f.slip-0000000000000000");
        create(staged.as_os_str().as_bytes(), 0o4755, b"post", &|_| {}).unwrap();
        assert_eq!(mode(&staged), 0o4755);
        assert_eq!(std::fs::read(&staged).unwrap(), b"post");
        std::fs::remove_dir_all(&r).unwrap();
    }
}
