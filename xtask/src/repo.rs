//! The files a command reads (`spec/harness.md` §3: "Lints read the files
//! tracked by git").
//!
//! In a git checkout (the root holds a `.git` directory or file, as in a
//! worktree) the list is `git ls-files`, so build output, worktrees and
//! untracked scratch files are never read; submodules are tracked as
//! gitlinks and their contents are not listed. A root that is not a git
//! checkout (a fixture tree made by a test) is walked instead, skipping
//! nothing but `.git`. Paths are relative to the root and `/`-separated.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The listed files of one repository root.
#[derive(Debug, Clone)]
pub struct Repo {
    root: PathBuf,
    files: BTreeSet<String>,
    gitlinks: BTreeSet<String>,
}

impl Repo {
    /// Lists the files under `root`; `Err` if `git ls-files` cannot run or
    /// fails in a git checkout, or the tree cannot be walked.
    pub fn open(root: &Path) -> Result<Repo, String> {
        let (files, gitlinks) = if root.join(".git").exists() {
            git_files(root)?
        } else {
            let mut files = BTreeSet::new();
            walk(root, "", &mut files)?;
            (files, BTreeSet::new())
        };
        Ok(Repo { root: root.to_path_buf(), files, gitlinks })
    }

    /// The root this repository was opened at.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every listed file, in byte order of the path.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(String::as_str)
    }

    /// Whether `rel` is a listed file.
    pub fn is_file(&self, rel: &str) -> bool {
        self.files.contains(rel)
    }

    /// Whether `rel` is a directory that contains a listed file or
    /// submodule (the root itself is `""`).
    pub fn is_dir(&self, rel: &str) -> bool {
        if rel.is_empty() {
            return true;
        }
        let prefix = format!("{rel}/");
        let under = |set: &BTreeSet<String>| {
            set.range(prefix.clone()..).next().is_some_and(|f| f.starts_with(&prefix))
        };
        under(&self.files) || under(&self.gitlinks) || self.gitlinks.contains(rel)
    }

    /// Whether `rel` is inside (or is) a submodule, whose contents git does
    /// not list; such a path is checked on disk.
    pub fn in_submodule(&self, rel: &str) -> bool {
        self.gitlinks.iter().any(|g| rel == g || rel.starts_with(&format!("{g}/")))
    }

    /// The listed files directly in directory `dir` (not in its
    /// subdirectories) whose name ends with `suffix`.
    pub fn files_in<'a>(&'a self, dir: &'a str, suffix: &'a str) -> impl Iterator<Item = &'a str> {
        self.files().filter(move |f| {
            f.strip_prefix(dir)
                .and_then(|r| r.strip_prefix('/'))
                .is_some_and(|name| !name.contains('/') && name.ends_with(suffix))
        })
    }

    /// The text of a file, `None` if it cannot be read or is not UTF-8.
    pub fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(rel)).ok()
    }
}

fn git_files(root: &Path) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    // `safe.directory`: a checkout owned by another user (a container's
    // bind mount) would otherwise be refused as "dubious ownership".
    let out = Command::new("git")
        .args(["-c", "safe.directory=*", "ls-files", "-z", "--stage"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run `git ls-files` in {}: {e}", root.display()))?;
    if !out.status.success() {
        return Err(format!(
            "`git ls-files` failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let mut files = BTreeSet::new();
    let mut gitlinks = BTreeSet::new();
    for entry in out.stdout.split(|&b| b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        // `<mode> <object> <stage>\t<path>`
        let Some((meta, path)) = entry.split_once('\t') else {
            return Err(format!("unexpected `git ls-files --stage` entry `{entry}`"));
        };
        if meta.starts_with("160000 ") {
            gitlinks.insert(path.to_owned());
        } else {
            files.insert(path.to_owned());
        }
    }
    Ok((files, gitlinks))
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeSet<String>) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let kind = entry.file_type().map_err(|e| format!("cannot stat {rel}: {e}"))?;
        // `file_type` does not follow symbolic links: a link is listed as
        // a file, so a link to a directory cannot make the walk loop.
        if kind.is_dir() {
            walk(&entry.path(), &rel, out)?;
        } else {
            out.insert(rel);
        }
    }
    Ok(())
}
