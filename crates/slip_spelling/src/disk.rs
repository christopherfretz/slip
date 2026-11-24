//! [`Disk`]: [`Lookup`] over `std::fs`, and which devices answer lookups
//! consistently, from `/proc/self/mountinfo`.

use std::collections::HashMap;
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

/// The mounts, by device: each device's file system types, one per
/// mountinfo line that names it.
#[derive(Debug, Default)]
pub struct Mounts {
    types: HashMap<u64, Vec<String>>,
}

impl Mounts {
    /// From the text of `/proc/<pid>/mountinfo`: field 3 is
    /// `major:minor`, and the field after the `-` that ends the optional
    /// fields is the type. A line not of that form is skipped.
    pub fn parse(text: &str) -> Mounts {
        let mut types: HashMap<u64, Vec<String>> = HashMap::new();
        for line in text.lines() {
            let fields: Vec<&str> = line.split(' ').collect();
            let Some((major, minor)) = fields.get(2).and_then(|f| f.split_once(':')) else {
                continue;
            };
            let (Ok(major), Ok(minor)) = (major.parse::<u64>(), minor.parse::<u64>()) else {
                continue;
            };
            let Some(dash) = fields.iter().skip(6).position(|f| *f == "-") else { continue };
            let Some(kind) = fields.get(6 + dash + 1) else { continue };
            types.entry(makedev(major, minor)).or_default().push((*kind).to_owned());
        }
        Mounts { types }
    }

    /// `/proc/self/mountinfo`; none when it cannot be read (not Linux).
    pub fn read() -> Mounts {
        fs::read_to_string("/proc/self/mountinfo").map(|t| Mounts::parse(&t)).unwrap_or_default()
    }

    /// The types of the mounts of device `dev`: empty when no line names it.
    pub fn types(&self, dev: u64) -> &[String] {
        self.types.get(&dev).map_or(&[], Vec::as_slice)
    }

    /// Consistent when some line names the device and every line's type
    /// is in [`CONSISTENT`].
    pub fn consistent(&self, dev: u64) -> bool {
        let t = self.types(dev);
        !t.is_empty() && t.iter().all(|t| CONSISTENT.contains(&t.as_str()))
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

    fn consistent(&self, dev: u64) -> bool {
        self.mounts().consistent(dev)
    }
}
