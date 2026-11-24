//! A front end over a few files written to a fresh directory, for tests
//! (work package WP-068, `spec/landed/edits/WP-068.md`).
//!
//! `slip_kiln::snippet` holds its files in memory: its buffers are not
//! files, so every token of it is `not_a_file` (`spec/edits.md` §2.3) and
//! has no `FileRef`. A [`Tree`] writes the files to disk, under a
//! directory no other value uses (tests run in parallel), and parses
//! them with the production parse ([`crate::parse_observer_with`]), so
//! that a unit's buffers are files with real paths, sizes and digests.
//! The directory is removed when the value is dropped, file by file and
//! directory by directory, by the paths it wrote: never by a pattern.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use slip_model::hash::content_digest;
use slip_model::{Command, FileRef, Language, Observer, Origin, Phase, Shard};

use crate::{ParseOptions, Unit, parse_observer_with};

/// Serialises the parses of every tree of the process: a parse changes
/// the process's current directory (`parse_observer_with`).
static PARSE: Mutex<()> = Mutex::new(());

/// Makes directory names unique within the process; the process id makes
/// them unique between processes.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory this value owns, holding the files it was given; removed
/// on drop, file by file.
#[derive(Debug)]
pub struct Tree {
    root: PathBuf,
    /// The files written, relative to `root`, with their bytes.
    files: Vec<(PathBuf, Vec<u8>)>,
    /// The directories created under `root`, parents before children.
    dirs: Vec<PathBuf>,
}

impl Tree {
    /// Writes `files` (relative path, bytes) under a new directory of
    /// `base` (a test passes `env!("CARGO_TARGET_TMPDIR")`), named after
    /// `name`, the process and a counter. `base` is created if missing;
    /// the root is its real path.
    ///
    /// # Errors
    ///
    /// A `name` that is not one normal path component (empty, `.`, `..`,
    /// or holding a `/`: the tree would not be a directory of `base`), or a
    /// path that is absolute, empty or has a `..` or `.` component
    /// (`InvalidInput`); two files with one path (`AlreadyExists`); any
    /// failure to create a directory or write a file. What was written
    /// before the failure is removed.
    pub fn new(base: &Path, name: &str, files: &[(&str, &[u8])]) -> std::io::Result<Tree> {
        let mut parts = Path::new(name).components();
        let one = matches!(parts.next(), Some(Component::Normal(c)) if c == name)
            && parts.next().is_none();
        if !one {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("not one normal path component: {name:?}"),
            ));
        }
        std::fs::create_dir_all(base)?;
        let root = loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = base.join(format!("{name}-{}-{n}", std::process::id()));
            match std::fs::create_dir(&dir) {
                Ok(()) => break dir,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        };
        let mut tree = Tree { root, files: Vec::new(), dirs: Vec::new() };
        tree.root = std::fs::canonicalize(&tree.root)?;
        for (path, bytes) in files {
            tree.add(path, bytes)?;
        }
        Ok(tree)
    }

    /// Writes one file, creating its parent directories.
    fn add(&mut self, path: &str, bytes: &[u8]) -> std::io::Result<()> {
        let rel = Path::new(path);
        let plain = rel.components().all(|c| matches!(c, Component::Normal(_)));
        if path.is_empty() || !plain {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("not a plain relative path: {path:?}"),
            ));
        }
        if self.files.iter().any(|(p, _)| p == rel) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("written twice: {path:?}"),
            ));
        }
        let mut dir = PathBuf::new();
        for c in rel.parent().into_iter().flat_map(Path::components) {
            dir.push(c);
            if !self.dirs.contains(&dir) {
                std::fs::create_dir(self.root.join(&dir))?;
                self.dirs.push(dir.clone());
            }
        }
        // Recorded before it is written, so a half-written file is
        // removed too.
        self.files.push((rel.to_owned(), bytes.to_vec()));
        std::fs::write(self.root.join(rel), bytes)
    }

    /// The tree's directory (a real path).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The observer of
    /// `kiln -std=<std> <args> -I<root> -I<root>/dep -c <main>` with
    /// directory `<root>`: the default command of
    /// `spec/harness.md` §6.2. Its `main` is the file's real path, size and
    /// digest as written, its `id` the command's `ObserverId`, its one
    /// origin `(0, 0)`.
    pub fn observer(&self, main: &str, std: &str, args: &[&str]) -> Observer {
        let root = self.root.to_string_lossy().into_owned();
        let mut words: Vec<String> = vec!["kiln".into(), format!("-std={std}")];
        words.extend(args.iter().map(|&a| a.to_owned()));
        words.extend([format!("-I{root}"), format!("-I{root}/dep"), "-c".into(), main.into()]);
        let command = Command {
            directory: root.clone().into_bytes(),
            argv: words.into_iter().map(String::into_bytes).collect(),
            env: Vec::new(),
        };
        let bytes = self.files.iter().find(|(p, _)| p == Path::new(main)).map(|(_, b)| b);
        let main = FileRef {
            path: format!("{root}/{main}").into_bytes(),
            size: bytes.map_or(0, |b| b.len() as u64),
            content: content_digest(bytes.map_or(&[][..], Vec::as_slice)),
        };
        Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            cost_hint: main.size,
            main,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }
    }

    /// `parse_observer_with` on `observer` for the map phase, round 1.
    /// Parses of all trees in one process are serialised by a lock here:
    /// the parse changes the working directory
    /// (`crates/slip_kiln/src/parse.rs`, "This changes the process's
    /// current directory").
    ///
    /// # Errors
    ///
    /// As `parse_observer_with`.
    pub fn parse<R: Send>(
        &self,
        observer: &Observer,
        options: &ParseOptions<'_>,
        f: impl FnOnce(&Unit<'_>) -> R + Send,
    ) -> Result<(Shard, Option<R>), String> {
        let _held = PARSE.lock().unwrap_or_else(PoisonError::into_inner);
        parse_observer_with(observer, Phase::Map, 1, *options, f)
    }
}

impl Drop for Tree {
    /// Removes each file written, then each directory created (children
    /// first), then the root. A file someone else put there is left, and
    /// so is every directory holding it; nothing fails.
    fn drop(&mut self) {
        for (p, _) in self.files.iter().rev() {
            let _ = std::fs::remove_file(self.root.join(p));
        }
        for d in self.dirs.iter().rev() {
            let _ = std::fs::remove_dir(self.root.join(d));
        }
        let _ = std::fs::remove_dir(&self.root);
    }
}
