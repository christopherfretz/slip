//! [`Disk`]: [`Lookup`] over `std::fs`, and which devices answer lookups
//! consistently, from `/proc/self/mountinfo`.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::OnceLock;

use crate::{Id, Lookup};

/// The file system types whose lookups are answered consistently: local
/// kernel file systems, whose answer is the directory's, every time
/// (`spec/edits.md` §2.1). Every other type is listed: FUSE (this
/// workspace's host mount answered a lookup in another case one time and
/// not the next, WP-075), NFS, CIFS, 9p, virtiofs, and `overlay` (its
/// answers are its layers', which mountinfo does not show cheaply).
/// Adding a type is safe where removing one is not.
pub const CONSISTENT: &[&str] = &["ext2", "ext3", "ext4", "xfs", "btrfs", "tmpfs", "f2fs"];

/// glibc's `makedev`: the `st_dev` of a file on the device `major:minor`.
pub fn makedev(major: u64, minor: u64) -> u64 {
    ((major & 0xffff_f000) << 32)
        | ((major & 0x0000_0fff) << 8)
        | ((minor & 0xffff_ff00) << 12)
        | (minor & 0x0000_00ff)
}

/// One line of mountinfo: where it is mounted, its device, its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// The mount point, unescaped (mountinfo writes a space, a tab, a
    /// newline and a backslash as `\ooo`).
    pub at: Vec<u8>,
    /// `makedev(major, minor)` of its `major:minor`.
    pub dev: u64,
    pub kind: String,
}

/// The mounts, in mountinfo's order (a later mount on one mount point
/// hides an earlier one).
#[derive(Debug, Default)]
pub struct Mounts {
    lines: Vec<Mount>,
}

/// `\ooo` octal escapes undone.
fn unescape(field: &str) -> Vec<u8> {
    let b = field.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = b[i + 1..i + 4].iter().fold(0u32, |v, c| v * 8 + u32::from(c - b'0'));
            if let Ok(byte) = u8::try_from(v) {
                out.push(byte);
                i += 4;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// `dir` is `at` or under it.
fn under(dir: &[u8], at: &[u8]) -> bool {
    at == b"/" || dir == at || (dir.starts_with(at) && dir.get(at.len()) == Some(&b'/'))
}

impl Mounts {
    /// From the text of `/proc/<pid>/mountinfo`: field 3 is
    /// `major:minor`, field 5 the mount point, and the field after the `-`
    /// that ends the optional fields is the type. A line not of that form
    /// is skipped.
    pub fn parse(text: &str) -> Mounts {
        let mut lines = Vec::new();
        for line in text.lines() {
            let fields: Vec<&str> = line.split(' ').collect();
            let Some((major, minor)) = fields.get(2).and_then(|f| f.split_once(':')) else {
                continue;
            };
            let (Ok(major), Ok(minor)) = (major.parse::<u64>(), minor.parse::<u64>()) else {
                continue;
            };
            let Some(at) = fields.get(4).filter(|f| f.starts_with('/')) else { continue };
            let Some(dash) = fields.iter().skip(6).position(|f| *f == "-") else { continue };
            let Some(kind) = fields.get(6 + dash + 1) else { continue };
            lines.push(Mount {
                at: unescape(at),
                dev: makedev(major, minor),
                kind: (*kind).to_owned(),
            });
        }
        Mounts { lines }
    }

    /// `/proc/self/mountinfo`; none when it cannot be read (not Linux).
    pub fn read() -> Mounts {
        fs::read_to_string("/proc/self/mountinfo").map(|t| Mounts::parse(&t)).unwrap_or_default()
    }

    /// The types of the mounts of device `dev`, one per line that names
    /// it: empty when none does.
    pub fn types(&self, dev: u64) -> Vec<&str> {
        self.lines.iter().filter(|m| m.dev == dev).map(|m| m.kind.as_str()).collect()
    }

    /// The mount that holds the directory `dir` (an absolute path): the
    /// line with the longest mount point that is `dir` or above it, the
    /// last of those (a later mount hides an earlier one on one point).
    pub fn holder(&self, dir: &[u8]) -> Option<&Mount> {
        let mut best: Option<&Mount> = None;
        for m in &self.lines {
            if under(dir, &m.at) && best.is_none_or(|b| m.at.len() >= b.at.len()) {
                best = Some(m);
            }
        }
        best
    }

    /// Whether lookups in directory `dir`, whose `st_dev` is `dev`, are
    /// answered consistently: the mount that holds it ([`Mounts::holder`])
    /// is of device `dev` and of a type in [`CONSISTENT`], and so is every
    /// other line that names `dev`. A file system may report a device of
    /// its own choosing (FUSE: this workspace's mount reports `st_dev` 45
    /// where its line says 0:47), so the device alone decides nothing; a
    /// directory no line holds, or whose holder names another device, is
    /// not trusted.
    pub fn consistent(&self, dir: &[u8], dev: u64) -> bool {
        let trusted = |m: &Mount| CONSISTENT.contains(&m.kind.as_str());
        self.holder(dir).is_some_and(|h| h.dev == dev && trusted(h))
            && self.lines.iter().filter(|m| m.dev == dev).all(trusted)
    }
}

/// The disk. The mounts are read once, on the first question.
#[derive(Debug, Default)]
pub struct Disk {
    mounts: OnceLock<Mounts>,
}

impl Disk {
    /// The mounts it decides by.
    pub fn mounts(&self) -> &Mounts {
        self.mounts.get_or_init(Mounts::read)
    }
}

fn path(p: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(p))
}

fn absent(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory)
}

impl Lookup for Disk {
    fn lstat(&self, p: &[u8]) -> io::Result<Option<Id>> {
        match fs::symlink_metadata(path(p)) {
            Ok(m) => Ok(Some(Id { dev: m.dev(), ino: m.ino() })),
            Err(e) if absent(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn names(&self, dir: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        let entries = match fs::read_dir(path(dir)) {
            Ok(e) => e,
            Err(e) if absent(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut names = Vec::new();
        for entry in entries {
            names.push(entry?.file_name().as_bytes().to_vec());
        }
        Ok(Some(names))
    }

    fn consistent(&self, dir: &[u8], dev: u64) -> bool {
        self.mounts().consistent(dir, dev)
    }
}
