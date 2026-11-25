//! Trees of files, judged byte for byte (`spec/harness.md` §6.2, §6.3):
//! reading one into memory (contents, mode bits, directories, and what is
//! neither), copying one, comparing two, and making and removing a test's
//! work directory by explicit path.

use std::collections::BTreeMap;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path, PathBuf};

/// One entry of a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A regular file: its permission bits (`mode & 0o7777`) and bytes.
    File { mode: u32, bytes: Vec<u8> },
    /// A directory (its mode is not compared: a copy's directories are
    /// made under the process's umask).
    Dir,
    /// A symbolic link and its target, which is not followed.
    Symlink(PathBuf),
    /// Anything else (a fifo, a socket, a device).
    Other,
}

impl Entry {
    /// The bytes of a regular file.
    pub fn bytes(&self) -> Option<&[u8]> {
        if let Entry::File { bytes, .. } = self { Some(bytes) } else { None }
    }

    /// Its kind, as messages name it.
    pub fn kind(&self) -> &'static str {
        match self {
            Entry::File { .. } => "file",
            Entry::Dir => "directory",
            Entry::Symlink(_) => "symbolic link",
            Entry::Other => "special file",
        }
    }
}

/// The entries of a tree, its root not included: path relative to the
/// root (components joined by `/`) to the entry, in byte order of paths.
pub type Files = BTreeMap<String, Entry>;

/// Every entry under `root`, directories walked (a symbolic link is an
/// entry and is not followed). A name that is not UTF-8 is an error.
pub fn read_tree(root: &Path) -> Result<Files, String> {
    let mut files = Files::new();
    walk(root, "", &mut files)?;
    Ok(files)
}

/// [`read_tree`] of a test's own input (`pre/`, `post/`): regular files
/// and directories only. A symbolic link or another kind of file is an
/// error (§6.2: a test's sources are plain files; a link could lead
/// `--bless` out of the test).
pub fn read_plain_tree(root: &Path) -> Result<Files, String> {
    match std::fs::symlink_metadata(root) {
        Ok(m) if m.is_dir() => {}
        Ok(m) if m.file_type().is_symlink() => return Err("a symbolic link".into()),
        Ok(_) => return Err("not a directory".into()),
        Err(e) => return Err(e.to_string()),
    }
    let files = read_tree(root)?;
    if let Some((p, e)) = files.iter().find(|(_, e)| !matches!(e, Entry::File { .. } | Entry::Dir))
    {
        return Err(format!("{p}: a {}, not a regular file or a directory", e.kind()));
    }
    Ok(files)
}

fn walk(dir: &Path, prefix: &str, files: &mut Files) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut list = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        list.push(entry);
    }
    list.sort_by_key(std::fs::DirEntry::file_name);
    for entry in list {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            return Err(format!("{}: the name is not UTF-8", path.display()));
        };
        let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let meta =
            std::fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let kind = meta.file_type();
        if kind.is_dir() {
            files.insert(rel.clone(), Entry::Dir);
            walk(&path, &rel, files)?;
        } else if kind.is_file() {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            files.insert(rel, Entry::File { mode: meta.permissions().mode() & 0o7777, bytes });
        } else if kind.is_symlink() {
            let target =
                std::fs::read_link(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            files.insert(rel, Entry::Symlink(target));
        } else {
            files.insert(rel, Entry::Other);
        }
    }
    Ok(())
}

/// Copies the tree `from` to `to` (which must not exist), file modes
/// included. Only regular files and directories are copied.
pub fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir(to)?;
    let mut list = Vec::new();
    for entry in std::fs::read_dir(from)? {
        list.push(entry?);
    }
    list.sort_by_key(std::fs::DirEntry::file_name);
    for entry in list {
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        } else {
            return Err(io::Error::other(format!(
                "{}: not a regular file or a directory",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

/// Creates the test's declared symbolic links (`spec/harness.md` §6.2) in
/// `root`, creating any missing parent directories. No hard link is made.
pub fn apply_symlinks(root: &Path, symlinks: &[super::test_file::Symlink]) -> io::Result<()> {
    for s in symlinks {
        let p = root.join(&s.link);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(&s.target, &p)?;
    }
    Ok(())
}

/// `pre` with the test's declared symbolic links (and any parent directories
/// they need) added.
pub fn with_symlinks(pre: &Files, symlinks: &[super::test_file::Symlink]) -> Files {
    let mut out = pre.clone();
    for s in symlinks {
        if let Some((dir, _)) = s.link.rsplit_once('/') {
            let mut prefix = String::new();
            for part in dir.split('/') {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                out.entry(prefix.clone()).or_insert(Entry::Dir);
            }
        }
        out.insert(s.link.clone(), Entry::Symlink(PathBuf::from(&s.target)));
    }
    out
}

/// Whether the volume holding `dir` folds ASCII case: probes `dir` if it
/// exists, else its deepest existing ancestor (without creating `dir`), by
/// creating a uniquely-named file, checking whether the upper-case spelling
/// names the same `(dev, ino)`, and removing the file by explicit path.
pub fn folds_case(dir: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt as _;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let probe_dir = dir
        .ancestors()
        .find(|p| p.is_dir())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no existing ancestor directory"))?;
    let tag =
        format!(".slip-case-probe.{}.{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
    let lower = probe_dir.join(format!("{tag}.a"));
    let upper = probe_dir.join(format!("{tag}.A"));
    std::fs::OpenOptions::new().write(true).create_new(true).open(&lower)?;
    let res = (|| -> io::Result<bool> {
        let ml = std::fs::symlink_metadata(&lower)?;
        match std::fs::symlink_metadata(&upper) {
            Ok(mu) => Ok(ml.dev() == mu.dev() && ml.ino() == mu.ino()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    })();
    let _ = std::fs::remove_file(&lower);
    res
}

/// The first difference of `actual` from `expected`, in byte order of
/// paths: `file <p> differs (first at byte <k>)`, `file <p> mode <m>,
/// expected <m>`, `<kind> <p> added`, `<kind> <p> missing`, `<kind> <p> is
/// now a <kind>`. `None` if they are equal.
pub fn first_difference(expected: &Files, actual: &Files) -> Option<String> {
    let mut paths: Vec<&String> = expected.keys().chain(actual.keys()).collect();
    paths.sort();
    paths.dedup();
    for p in paths {
        match (expected.get(p), actual.get(p)) {
            (Some(e), Some(a)) if e == a => {}
            (
                Some(Entry::File { mode: em, bytes: e }),
                Some(Entry::File { mode: am, bytes: a }),
            ) => {
                if e != a {
                    let k = e.iter().zip(a.iter()).take_while(|(x, y)| x == y).count();
                    return Some(format!("file {p} differs (first at byte {k})"));
                }
                return Some(format!("file {p} mode {am:o}, expected {em:o}"));
            }
            (Some(Entry::Symlink(e)), Some(Entry::Symlink(a))) => {
                return Some(format!(
                    "symbolic link {p} leads to {}, expected {}",
                    a.display(),
                    e.display()
                ));
            }
            (Some(e), Some(a)) => {
                return Some(format!("{} {p} is now a {}", e.kind(), a.kind()));
            }
            (None, Some(a)) => return Some(format!("{} {p} added", a.kind())),
            (Some(e), None) => return Some(format!("{} {p} missing", e.kind())),
            (None, None) => {}
        }
    }
    None
}

/// `pre` with the entries of `post` laid over it (§6.2).
pub fn overlay(pre: &Files, post: &Files) -> Files {
    let mut out = pre.clone();
    for (p, b) in post {
        out.insert(p.clone(), b.clone());
    }
    out
}

/// The paths of regular files of `after` that are new or whose bytes or
/// mode differ from `before`.
pub fn changed_files(before: &Files, after: &Files) -> Vec<String> {
    after
        .iter()
        .filter(|(p, e)| matches!(e, Entry::File { .. }) && before.get(*p) != Some(e))
        .map(|(p, _)| p.clone())
        .collect()
}

/// Checks that `work` is a test's own work directory,
/// `<target>/transform/<area>/<name>` or
/// `$SLIP_TEST_ROOT/transform/<checkout>/<area>/<name>` (absolute, no `.`
/// or `..` component, its last two components `area` and `name` and the
/// third or fourth from the end `transform`; [`crate::transform::work_base`]),
/// and that it neither holds nor lies inside `test_dir`. The work directory
/// is removed by this path, so nothing else may pass.
pub fn check_work_dir(work: &Path, area: &str, name: &str, test_dir: &Path) -> Result<(), String> {
    let shown = work.display();
    if !work.is_absolute() {
        return Err(format!("the work directory {shown} is not absolute"));
    }
    let parts: Vec<Component<'_>> = work.components().collect();
    if parts.iter().any(|c| matches!(c, Component::CurDir | Component::ParentDir)) {
        return Err(format!("the work directory {shown} has a `.` or `..` component"));
    }
    let names: Vec<&std::ffi::OsStr> = parts
        .iter()
        .filter_map(|c| if let Component::Normal(n) = c { Some(*n) } else { None })
        .collect();
    let n = names.len();
    let ours = n >= 4
        && names[n - 2] == std::ffi::OsStr::new(area)
        && names[n - 1] == std::ffi::OsStr::new(name)
        && (names[n - 3] == "transform" || (n >= 5 && names[n - 4] == "transform"));
    if !ours {
        return Err(format!(
            "the work directory {shown} is not under <target>/transform/{area}/{name} \
             or $SLIP_TEST_ROOT/transform/<checkout>/{area}/{name}"
        ));
    }
    let test_abs = std::path::absolute(test_dir).unwrap_or_else(|_| test_dir.to_owned());
    if work.starts_with(&test_abs) || test_abs.starts_with(work) {
        return Err(format!("the work directory {shown} and the test directory overlap"));
    }
    Ok(())
}

/// Removes `path` if it exists: a directory with everything in it, or a
/// file or symbolic link (which is not followed).
pub fn remove(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Removes and makes again the work directory, after [`check_work_dir`];
/// returns its canonical path.
pub fn fresh_dir(work: &Path) -> io::Result<PathBuf> {
    remove(work)?;
    std::fs::create_dir_all(work)?;
    work.canonicalize()
}
