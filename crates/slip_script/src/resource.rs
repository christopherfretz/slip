//! The resource headers of the transform API (`spec/api.md` §2.1, §2.2,
//! §8.2): `<slip/meta.h>` and `<slip/__intrinsics.h>` as this build ships
//! them, where the running Slip finds them, and the header check
//! (`transform-header`).
//!
//! The two files under the repository's `resource/include/slip/` are
//! normative and embedded here byte for byte; the host accepts a transform
//! unit only if it read exactly these bytes from `<resource>/include/slip/`.
//! Decisions the specification leaves open are in
//! `spec/landed/api/WP-082.md`.

use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use slip_model::Digest;
use slip_model::hash::content_digest;

/// `<slip/meta.h>`, layer L1, exactly as shipped.
pub const META_H: &[u8] = include_bytes!("../../../resource/include/slip/meta.h");

/// `<slip/__intrinsics.h>`, layer L0, exactly as shipped.
pub const INTRINSICS_H: &[u8] = include_bytes!("../../../resource/include/slip/__intrinsics.h");

/// The API level (`spec/api.md` §8.2): the value of `__slip_lib_meta`.
pub const API_LEVEL: u64 = 202611;

/// The variable that names the resource directory (§2.1 rule 1).
pub const RESOURCE_DIR_VAR: &str = "SLIP_RESOURCE_DIR";

/// The embedded files: each one's path below the resource directory, its
/// name in errors, and its bytes. `meta.h` first.
pub const HEADERS: [Header; 2] = [
    Header { path: "include/slip/meta.h", name: "slip/meta.h", bytes: META_H },
    Header {
        path: "include/slip/__intrinsics.h",
        name: "slip/__intrinsics.h",
        bytes: INTRINSICS_H,
    },
];

/// One embedded file of the resource directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Its path relative to the resource directory, `/`-separated.
    pub path: &'static str,
    /// The name it is included by (`slip/meta.h`), used in errors.
    pub name: &'static str,
    pub bytes: &'static [u8],
}

impl Header {
    /// The content digest (`spec/model.md` §3) of the embedded bytes.
    pub fn digest(&self) -> Digest {
        static DIGESTS: OnceLock<[Digest; 2]> = OnceLock::new();
        let digests = DIGESTS
            .get_or_init(|| [content_digest(HEADERS[0].bytes), content_digest(HEADERS[1].bytes)]);
        match HEADERS.iter().position(|h| h == self) {
            Some(i) => digests[i],
            None => content_digest(self.bytes),
        }
    }

    /// Its path under `resource`.
    pub fn path_in(&self, resource: &Path) -> PathBuf {
        self.path.split('/').fold(resource.to_path_buf(), |p, c| p.join(c))
    }
}

/// The content digest of [`META_H`].
pub fn meta_h_digest() -> Digest {
    HEADERS[0].digest()
}

/// The content digest of [`INTRINSICS_H`].
pub fn intrinsics_h_digest() -> Digest {
    HEADERS[1].digest()
}

/// The installed resource directory relative to the executable's directory.
const INSTALLED: &str = "../share/slip/resource";

/// The repository's `resource/`, for a build that is not installed.
const REPOSITORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resource");

/// The resource directory of a Slip whose executable is `exe`, given the
/// value of `$SLIP_RESOURCE_DIR` (`env`):
///
/// 1. `env`, if it is set and not empty, as given;
/// 2. else `<directory of exe>/../share/slip/resource`, if that is a
///    directory;
/// 3. else the repository's `resource/` this crate was built from, if that
///    is a directory (a development build);
/// 4. else the path of 2, so that an error names where an installed Slip
///    looks.
///
/// No path is canonicalised or made absolute.
pub fn resource_dir(exe: &Path, env: Option<&OsStr>) -> PathBuf {
    if let Some(dir) = env.filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let installed = exe.parent().unwrap_or_else(|| Path::new("")).join(INSTALLED);
    if installed.is_dir() {
        return installed;
    }
    let repository = Path::new(REPOSITORY);
    if repository.is_dir() {
        return repository.to_path_buf();
    }
    installed
}

/// Writes the embedded files under `dir` (`slip resource --write`),
/// creating the directories needed. Each file is written to a temporary
/// name beside it and renamed into place, so a reader sees the old bytes or
/// the new ones. An error names the path it concerns.
pub fn write_to(dir: &Path) -> io::Result<()> {
    for header in &HEADERS {
        let path = header.path_in(dir);
        let parent = path.parent().unwrap_or(dir);
        std::fs::create_dir_all(parent).map_err(|e| at(parent, &e))?;
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
        if let Err(e) = std::fs::write(&temporary, header.bytes) {
            let _ = std::fs::remove_file(&temporary);
            return Err(at(&temporary, &e));
        }
        if let Err(e) = std::fs::rename(&temporary, &path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(at(&path, &e));
        }
    }
    Ok(())
}

fn at(path: &Path, e: &io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

/// Why a transform unit is refused (`transform-header`, §2.2). Each names
/// the header (`slip/meta.h` or `slip/__intrinsics.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderError {
    /// The unit did not read the header from `<resource>/include/slip/`.
    NotRead(&'static str),
    /// It read it there, and the bytes are not this build's.
    Differs(&'static str),
}

impl HeaderError {
    /// The error's name in `spec/api.md` §12.
    pub const NAME: &'static str = "transform-header";
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderError::NotRead(name) => write!(
                f,
                "transform-header: the transform unit did not read <{name}> from the resource \
                 directory of this Slip"
            ),
            HeaderError::Differs(name) => write!(
                f,
                "transform-header: the <{name}> the transform unit read is not the one this Slip \
                 was built with"
            ),
        }
    }
}

impl std::error::Error for HeaderError {}

/// The header check (§2.2): `read` is the files a transform unit read, as
/// (path as given, content digest) pairs; both embedded headers must be
/// among them at their path under `resource`, with this build's digests.
/// `meta.h` is checked first. Paths are compared as paths (component by
/// component, `spec/landed/api/WP-082.md`), never resolved.
pub fn check_headers(read: &[(Vec<u8>, Digest)], resource: &Path) -> Result<(), HeaderError> {
    for header in &HEADERS {
        let expected = header.path_in(resource);
        let mut found = false;
        for (path, digest) in read {
            if path_from_bytes(path) != expected {
                continue;
            }
            found = true;
            if *digest != header.digest() {
                return Err(HeaderError::Differs(header.name));
            }
        }
        if !found {
            return Err(HeaderError::NotRead(header.name));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}
