//! `cargo xtask lint` (`spec/harness.md` §3): every lint over the files of
//! one repository root, each violation reported as
//! `<path>[:<line>]: <lint>: <message>`.
//!
//! Lints run in the order of the table in §3; within a lint, violations are
//! ordered by path and line. The exit status is 0 if there is none, 1 if
//! there is any or the files cannot be listed, 2 for a usage error.

pub(crate) mod api_tests;
mod crates;
mod doc_links;
mod local_paths;
pub(crate) mod manifest;
mod real_path;
mod rust_tokens;
mod specs;
mod swallowed_result;
mod wp_files;

pub use crates::KILN_LINKING;
pub use local_paths::local_paths;

use crate::repo::Repo;
use std::fmt;
use std::path::Path;
use std::process::ExitCode;

/// The lint names, in the order they run (§3).
pub const LINTS: &[&str] = &[
    "kiln-boundary",
    "forbid-unsafe",
    "dependencies",
    "spec-index",
    "spec-shape",
    "landed",
    "wp",
    "local-paths",
    "doc-links",
    "api-tests",
    "real-path",
    "swallowed-result",
];

/// One violation of one lint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The file, relative to the repository root, `/`-separated.
    pub path: String,
    /// The 1-based line, when the violation is on one.
    pub line: Option<usize>,
    /// The lint's name, one of [`LINTS`].
    pub lint: &'static str,
    pub message: String,
}

impl Violation {
    pub fn new(
        path: impl Into<String>,
        at: Option<usize>,
        lint: &'static str,
        message: impl Into<String>,
    ) -> Violation {
        Violation { path: path.into(), line: at, lint, message: message.into() }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{line}: {}: {}", self.path, self.lint, self.message),
            None => write!(f, "{}: {}: {}", self.path, self.lint, self.message),
        }
    }
}

/// Every violation of every lint in the repository at `root`; `Err` if its
/// files cannot be listed.
pub fn run(root: &Path) -> Result<Vec<Violation>, String> {
    Ok(check(&Repo::open(root)?))
}

/// Every violation of every lint in `repo`.
pub fn check(repo: &Repo) -> Vec<Violation> {
    let groups = [
        crates::kiln_boundary(repo),
        crates::forbid_unsafe(repo),
        crates::dependencies(repo),
        specs::spec_index(repo),
        specs::spec_shape(repo),
        specs::landed(repo),
        wp_files::wp(repo),
        local_paths::check(repo),
        doc_links::check(repo),
        api_tests::check(repo),
        real_path::check(repo),
        swallowed_result::check(repo),
    ];
    let mut all = Vec::new();
    for mut group in groups {
        group.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
        all.extend(group);
    }
    all
}

/// `cargo xtask lint`: prints every violation to stdout, one per line.
pub fn main(root: &Path, args: &[String]) -> ExitCode {
    if let Some(arg) = args.first() {
        eprintln!("xtask lint: unexpected argument `{arg}`\nusage: cargo xtask lint");
        return ExitCode::from(2);
    }
    match run(root) {
        Ok(violations) if violations.is_empty() => ExitCode::SUCCESS,
        Ok(violations) => {
            for v in &violations {
                println!("{v}");
            }
            eprintln!("xtask lint: {} violation(s)", violations.len());
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("xtask lint: {e}");
            ExitCode::from(1)
        }
    }
}

/// The lines of `text` outside fenced code blocks, each with its 1-based
/// number.
pub(crate) fn prose_lines(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(&t[..3]);
            continue;
        }
        out.push((i + 1, line));
    }
    out
}
