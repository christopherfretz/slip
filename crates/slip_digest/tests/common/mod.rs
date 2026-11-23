//! What the pair-test runners share (WP-024, `spec/digest.md` §14,
//! `spec/digest-kinds.md` §1.5): reading a test directory, parsing a side
//! with `slip_kiln::snippet`, digesting it, and reading `expect.txt`.
//!
//! A directory is `tests/<root>/<short>/<row>/` or `tests/<root>/_/<slug>/`.
//! It holds `t.cpp` (which begins `#include "side.h"`; the runner supplies
//! `side.h`, empty for side A and `#define B 1` for side B) or `a.cpp` and
//! `b.cpp`; any other file is a header both sides see. The main file's
//! header lines ([`header`]) choose the standard, the driver arguments
//! and the target it is parsed under (`slip_kiln::snippet::parse_with`)
//! and whether errors are allowed. Both sides' main file is parsed under
//! the path `main.cpp`, so that their groups have the same subject path.

#![allow(dead_code, reason = "each runner uses a part")]

pub mod pairs;
pub mod rules;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use slip_digest::{Cause, Detail, Input, Options, Output};
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::RuleTable;

/// Which side of a pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// One unit of a digested side, by key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitInfo {
    pub class: GroupClass,
    pub digest: Digest,
    pub flags: u64,
    pub name: String,
    pub cause: Option<Cause>,
}

/// A digested side.
#[derive(Debug)]
pub struct Digested {
    pub output: Output,
    /// Every unit by key; a key with several units has all of them.
    pub units: BTreeMap<Digest, Vec<UnitInfo>>,
    pub errors: u32,
    pub diagnostics: String,
}

impl Digested {
    /// The name of a key (the first of its units).
    pub fn name(&self, k: &Digest) -> String {
        self.units
            .get(k)
            .and_then(|v| v.first())
            .map_or_else(|| format!("{k:?}"), |u| u.name.clone())
    }
}

/// The crate directory.
pub fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every test directory under `tests/<root>`, sorted: `<short>/<row>`
/// and `_/<slug>`. [`run_all`] alone narrows them (`SLIP_DIGEST_ONLY`).
pub fn dirs(root: &str) -> Vec<PathBuf> {
    let base = manifest().join("tests").join(root);
    let mut out = Vec::new();
    let Ok(shorts) = std::fs::read_dir(&base) else { return out };
    for short in shorts.flatten() {
        if !short.path().is_dir() {
            continue;
        }
        for row in std::fs::read_dir(short.path()).into_iter().flatten().flatten() {
            if row.path().is_dir() {
                out.push(row.path());
            }
        }
    }
    out.sort();
    out
}

/// The directory relative to `tests/`, for messages.
pub fn rel(dir: &Path) -> String {
    dir.strip_prefix(manifest().join("tests")).unwrap_or(dir).display().to_string()
}

/// The files of a test directory, by name.
fn files(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        if e.path().is_file() {
            let name = e.file_name().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(e.path()).map_err(|e| format!("{name}: {e}"))?;
            out.insert(name, text);
        }
    }
    Ok(out)
}

/// What the header lines of a main file say (WP-038,
/// `spec/landed/harness/WP-038.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// `// std: X`; `c++20` without one.
    pub std: String,
    /// `// errors: yes`.
    pub errors: bool,
    /// `// args: A B …`, split at spaces; none without one.
    pub args: Vec<String>,
    /// `// target: T`; `None` is the snippet front end's default
    /// (`x86_64-unknown-linux-gnu`).
    pub target: Option<String>,
    /// `// module: <name>=<file>`, once per module unit the main file may
    /// import, in order (WP-048, `spec/landed/harness/WP-048.md`): the
    /// module (`m`, `m.n`, a partition `m:p`) and the file of the test
    /// directory that is its interface source.
    pub modules: Vec<(String, String)>,
}

impl Header {
    /// The command line the side is parsed under, as `kiln` would be
    /// given it (for `SLIP_DIGEST_DUMP` and messages).
    pub fn line(&self) -> String {
        let mut line = vec![format!("-std={}", self.std)];
        line.extend(self.target.iter().map(|t| format!("--target={t}")));
        line.extend(self.args.iter().cloned());
        line.extend(self.modules.iter().map(|(n, f)| format!("-fmodule-file={n}={f}")));
        line.push("-fsyntax-only".to_owned());
        line.join(" ")
    }
}

/// The keys of the header lines.
const HEADER_KEYS: [&str; 5] = ["std", "errors", "args", "target", "module"];

/// Whether `name` is a module name as `// module:` gives it: identifiers
/// (ASCII letters, digits and `_`, not beginning with a digit) joined by
/// `.`, then optionally `:` and a partition name of the same form.
fn module_name(name: &str) -> bool {
    let dotted = |s: &str| {
        s.split('.').all(|id| {
            id.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    };
    match name.split_once(':') {
        Some((m, p)) => dotted(m) && dotted(p),
        None => dotted(name),
    }
}

/// Whether `line` looks like a header line: `//`, any blanks, a word of
/// ASCII letters and `-`, any blanks, then `:`. A comment that reads so
/// (`// Note: …`) cannot open a main file.
fn looks_like_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("//") else { return false };
    let rest = rest.trim_start();
    let word =
        rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == '-').len();
    word > 0
        && rest[..word].starts_with(|c: char| c.is_ascii_alphabetic())
        && rest[word..].trim_start().starts_with(':')
}

/// Whether `line` is exactly a header line of one of the keys (the value
/// not checked further), after a byte-order mark if any.
fn is_header_line(line: &str) -> bool {
    let line = line.trim_start_matches('\u{feff}');
    line.strip_prefix("// ").and_then(|r| r.split_once(": ")).is_some_and(|(key, value)| {
        HEADER_KEYS.contains(&key) && !value.is_empty() && (key != "errors" || value == "yes")
    })
}

/// The header lines of a main file: the lines from its top that are
/// header lines or `#include "side.h"` (a `t.cpp` begins with it), up to
/// the first that is neither. A header line after that (after a blank
/// line, a comment, a byte-order mark, a `/* */`) is an error of the
/// test: it would not be read.
///
/// A header line is exactly `// <key>: <value>` (one space after `//`
/// and after the colon, the value not empty) with the key `std`,
/// `errors` (value `yes`), `args` (split at spaces; no quoting) or
/// `target`, each at most once, and `module` (`<name>=<file>`, a module
/// name and a file name, once per module unit; a name or a file twice is
/// an error). A line of the block that looks like a
/// header line ([`looks_like_header`]) but is not one (`// arg: x`,
/// `//target: x`, `// errors: no`, a key twice) is an error of the test.
pub fn header(main: &str) -> Result<Header, String> {
    let mut h = Header {
        std: "c++20".to_owned(),
        errors: false,
        args: Vec::new(),
        target: None,
        modules: Vec::new(),
    };
    let mut seen: Vec<&str> = Vec::new();
    let mut lines = main.lines().enumerate();
    for (i, line) in lines.by_ref() {
        let line = line.trim_end();
        if line == "#include \"side.h\"" {
            continue;
        }
        if !looks_like_header(line) {
            if is_header_line(line) {
                return Err(format!("line {}: header line outside the header block", i + 1));
            }
            break;
        }
        let bad = || {
            format!(
                "line {}: `{line}` looks like a header line but is not `// std: <std>`, \
                 `// errors: yes`, `// args: <arguments>`, `// target: <triple>` or \
                 `// module: <name>=<file>`",
                i + 1
            )
        };
        let (key, value) =
            line.strip_prefix("// ").and_then(|r| r.split_once(": ")).ok_or_else(bad)?;
        if !HEADER_KEYS.contains(&key) || value.is_empty() || value.starts_with(' ') {
            return Err(bad());
        }
        if key != "module" && seen.contains(&key) {
            return Err(format!("line {}: a second `// {key}:` line", i + 1));
        }
        seen.push(key);
        match key {
            "std" => value.clone_into(&mut h.std),
            "errors" if value == "yes" => h.errors = true,
            "args" => {
                h.args = value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect();
            }
            "target" if !value.contains(' ') => h.target = Some(value.to_owned()),
            "module" => {
                let Some((name, file)) = value.split_once('=') else { return Err(bad()) };
                if !module_name(name) || file.is_empty() || file.contains([' ', '=']) {
                    return Err(bad());
                }
                if h.modules.iter().any(|(n, _)| n == name) {
                    return Err(format!("line {}: module `{name}` is given twice", i + 1));
                }
                if h.modules.iter().any(|(_, f)| f == file) {
                    return Err(format!("line {}: file `{file}` is given twice", i + 1));
                }
                h.modules.push((name.to_owned(), file.to_owned()));
            }
            _ => return Err(bad()),
        }
    }
    if let Some((i, _)) = lines.find(|(_, l)| is_header_line(l.trim_end())) {
        return Err(format!("line {}: header line outside the header block", i + 1));
    }
    Ok(h)
}

/// Whether the test is marked `# pending` in its `expect.txt`: the
/// runner skips it and lists it.
pub fn is_pending(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("expect.txt"))
        .is_ok_and(|t| t.lines().any(|l| l.trim() == "# pending"))
}

/// Parses and digests one side of the test in `dir`.
pub fn digest_side(dir: &Path, side: Side) -> Result<Digested, String> {
    digest_side_with(dir, side, &RuleTable::empty())
}

/// [`digest_side`] under the rules `rules`.
pub fn digest_side_with(dir: &Path, side: Side, rules: &RuleTable) -> Result<Digested, String> {
    let (list, head) = side_files(&files(dir)?, side)?;
    // `// module:` (WP-048): each file is another file of the snippet.
    for (name, file) in &head.modules {
        let main = matches!(file.as_str(), "main.cpp" | "t.cpp" | "a.cpp" | "b.cpp" | "side.h");
        if main || !list.iter().any(|(n, _)| n == file) {
            return Err(format!(
                "side {side:?}: module `{name}`: `{file}` is not another file of the test"
            ));
        }
    }
    if let Some((name, file)) = head.modules.first() {
        // `spec/landed/harness/WP-048.md`: Kiln imports a named module
        // only from a file on disk, and a snippet has none.
        return Err(format!(
            "side {side:?} (`kiln {}`): module `{name}` (`{file}`): the snippet front end \
             cannot import a named module from memory yet (docs/requests/KF5)",
            head.line()
        ));
    }
    let refs: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let snippet = slip_kiln::snippet::Snippet {
        files: &refs,
        main: "main.cpp",
        include: &[""],
        std: &head.std,
    };
    let args: Vec<&str> = head.args.iter().map(String::as_str).collect();
    let extra = slip_kiln::snippet::Extra { args: &args, target: head.target.as_deref() };
    let out = slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
        // `Parsed` is not `Sync`: the walk's thread gets the AST, the
        // source manager and the paths.
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let (ast, sm) = (p.ast, p.sm);
        let output = on_walk_stack(|| {
            let path = |f: slip_kiln::kiln::source::FileId| {
                paths.get(f.index()).cloned().unwrap_or_default()
            };
            let input = Input { ast, sm, subject_path: &path };
            let options = Options {
                rules,
                read_only: slip_digest::ReadOnly::NO_FILE,
                detail: Detail::All,
                names: true,
            };
            slip_digest::digest(&input, &options)
        });
        (output, p.errors, p.diagnostics.clone())
    })
    .map_err(|e| format!("side {side:?} (`kiln {}`): {e}", head.line()))?;
    let (output, errors, diagnostics) = out;
    if errors > 0 && !head.errors {
        return Err(format!(
            "side {side:?} (`kiln {}`) has {errors} error(s):\n{diagnostics}",
            head.line()
        ));
    }
    let units = index(&output);
    if std::env::var_os("SLIP_DIGEST_DUMP").is_some() {
        eprintln!("--- {} side {side:?}: kiln {}", rel(dir), head.line());
        for (k, v) in &units {
            for u in v {
                eprintln!(
                    "  {:?} {} {:?} flags={} {:?}",
                    u.class,
                    u.name,
                    &k.0[..4],
                    u.flags,
                    u.cause
                );
            }
        }
        for c in &output.counters {
            eprintln!("  counter {c:?}");
        }
    }
    Ok(Digested { output, units, errors, diagnostics })
}

/// Parses one side of the test in `dir` as [`digest_side_with`] does and
/// runs `f` on its input, on the walk's stack, so that a runner can walk
/// one parse several times (WP-103: records, and the post-image under
/// several tables). An error of the side is an `Err` as there.
pub fn with_side<R: Send>(
    dir: &Path,
    side: Side,
    f: impl FnOnce(&Input<'_>) -> R + Send,
) -> Result<R, String> {
    let (list, head) = side_files(&files(dir)?, side)?;
    let refs: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let snippet = slip_kiln::snippet::Snippet {
        files: &refs,
        main: "main.cpp",
        include: &[""],
        std: &head.std,
    };
    let args: Vec<&str> = head.args.iter().map(String::as_str).collect();
    let extra = slip_kiln::snippet::Extra { args: &args, target: head.target.as_deref() };
    let out = slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let (ast, sm) = (p.ast, p.sm);
        let r = on_walk_stack(|| {
            let path = |f: slip_kiln::kiln::source::FileId| {
                paths.get(f.index()).cloned().unwrap_or_default()
            };
            let input = Input { ast, sm, subject_path: &path };
            f(&input)
        });
        (r, p.errors, p.diagnostics.clone())
    })
    .map_err(|e| format!("side {side:?} (`kiln {}`): {e}", head.line()))?;
    let (r, errors, diagnostics) = out;
    if errors > 0 && !head.errors {
        return Err(format!(
            "side {side:?} (`kiln {}`) has {errors} error(s):\n{diagnostics}",
            head.line()
        ));
    }
    Ok(r)
}

/// A side as [`digest_side_with`] gives it, from an `Output` a caller of
/// [`with_side`] made.
pub fn digested(output: Output) -> Digested {
    let units = index(&output);
    Digested { output, units, errors: 0, diagnostics: String::new() }
}

/// The files of `tests/support/`, by name: files of every snippet whose
/// directory has no file of that name (the `std::meta` declarations of
/// `meta.h` for the reflection tests). A file no snippet includes is
/// never read by the front end.
fn support() -> &'static BTreeMap<String, String> {
    static FILES: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    FILES.get_or_init(|| {
        let dir = manifest().join("tests").join("support");
        let mut out = BTreeMap::new();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(e.path())
                .unwrap_or_else(|err| panic!("{}: {err}", e.path().display()));
            out.insert(name, text);
        }
        out
    })
}

/// The snippet files of one side of a test whose files are `all`, and
/// the header lines its main file says: a `t.cpp` is both sides' main
/// file (with the side's `side.h`), so both read its lines; an
/// `a.cpp` / `b.cpp` test's sides each read their own.
pub fn side_files(
    all: &BTreeMap<String, String>,
    side: Side,
) -> Result<(Vec<(String, String)>, Header), String> {
    let mut list: Vec<(String, String)> = Vec::new();
    let main_text;
    if let Some(t) = all.get("t.cpp") {
        main_text = t.clone();
        let side_h = match side {
            Side::A => "",
            Side::B => "#define B 1\n",
        };
        list.push(("side.h".to_owned(), side_h.to_owned()));
    } else {
        let name = match side {
            Side::A => "a.cpp",
            Side::B => "b.cpp",
        };
        main_text = all.get(name).cloned().ok_or_else(|| format!("no t.cpp and no {name}"))?;
    }
    list.push(("main.cpp".to_owned(), main_text.clone()));
    for (name, text) in all {
        if !matches!(name.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
            list.push((name.clone(), text.clone()));
        }
    }
    for (name, text) in support() {
        if !all.contains_key(name) {
            list.push((name.clone(), text.clone()));
        }
    }
    let head = header(&main_text).map_err(|e| format!("side {side:?}: {e}"))?;
    Ok((list, head))
}

/// Runs `f` (a walk) on a thread of the stack `slip_digest::digest`
/// documents it needs (`slip_digest::STACK_SIZE`, `lib.rs` "What bounds
/// the stack"), as a caller of the walk must.
pub fn on_walk_stack<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("walk".into())
            .stack_size(slip_digest::STACK_SIZE)
            .spawn_scoped(scope, f)
            .expect("a thread for the walk")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

/// Every unit of an output by key.
pub fn index(o: &Output) -> BTreeMap<Digest, Vec<UnitInfo>> {
    let names: BTreeMap<Digest, String> = o.names.iter().cloned().collect();
    let causes: BTreeMap<Digest, Cause> = o.causes.iter().copied().collect();
    let mut out: BTreeMap<Digest, Vec<UnitInfo>> = BTreeMap::new();
    for g in &o.digests.groups {
        for u in g.units.iter().flatten() {
            out.entry(u.key).or_default().push(UnitInfo {
                class: g.class,
                digest: u.digest,
                flags: u.flags,
                name: names.get(&u.key).cloned().unwrap_or_default(),
                cause: causes.get(&u.key).copied(),
            });
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|a| (a.class, a.digest, a.flags));
    }
    out
}

/// One line of `expect.txt`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Expect {
    Changed(String),
    Added(String),
    Removed(String),
    Indeterminate(String),
    /// The derived units under a key of the name differ in any way,
    /// presence included; under `exact derived` (and in
    /// [`derived_differences`]), both sides have one and they differ.
    DerivedChanged(String),
    /// Side B has a derived unit under a key of the name, side A none.
    DerivedAdded(String),
    /// Side A has a derived unit under a key of the name, side B none.
    DerivedRemoved(String),
}

impl Expect {
    /// The name the line holds.
    pub fn name(&self) -> &str {
        match self {
            Self::Changed(n)
            | Self::Added(n)
            | Self::Removed(n)
            | Self::Indeterminate(n)
            | Self::DerivedChanged(n)
            | Self::DerivedAdded(n)
            | Self::DerivedRemoved(n) => n,
        }
    }

    /// The line as `expect.txt` writes it.
    pub fn line(&self) -> String {
        let word = match self {
            Self::Changed(_) => "changed",
            Self::Added(_) => "added",
            Self::Removed(_) => "removed",
            Self::Indeterminate(_) => "indeterminate",
            Self::DerivedChanged(_) => "derived-changed",
            Self::DerivedAdded(_) => "derived-added",
            Self::DerivedRemoved(_) => "derived-removed",
        };
        format!("{word} {}", self.name())
    }

    /// Whether it is a `derived-*` line.
    pub fn is_derived(&self) -> bool {
        matches!(self, Self::DerivedChanged(_) | Self::DerivedAdded(_) | Self::DerivedRemoved(_))
    }
}

/// The directive `exact derived` (`spec/landed/harness/WP-047.md`).
const EXACT_DERIVED: &str = "exact derived";

/// `expect.txt` read: its lines, and whether it has `exact derived`.
fn read_expect(dir: &Path) -> Result<(BTreeSet<Expect>, bool), String> {
    let Ok(text) = std::fs::read_to_string(dir.join("expect.txt")) else {
        return Ok((BTreeSet::new(), false));
    };
    let mut out = BTreeSet::new();
    let mut exact = false;
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == EXACT_DERIVED {
            if exact {
                return Err(format!("expect.txt:{}: a second `{EXACT_DERIVED}`", i + 1));
            }
            exact = true;
            continue;
        }
        let (word, name) =
            line.split_once(' ').ok_or_else(|| format!("expect.txt:{}: no name", i + 1))?;
        let name = name.trim().to_owned();
        let e = match word {
            "changed" => Expect::Changed(name),
            "added" => Expect::Added(name),
            "removed" => Expect::Removed(name),
            "indeterminate" => Expect::Indeterminate(name),
            "derived-changed" => Expect::DerivedChanged(name),
            "derived-added" => Expect::DerivedAdded(name),
            "derived-removed" => Expect::DerivedRemoved(name),
            _ => return Err(format!("expect.txt:{}: unknown word `{word}`", i + 1)),
        };
        if out.contains(&e) {
            return Err(format!("expect.txt:{}: `{}` a second time", i + 1, e.line()));
        }
        out.insert(e);
    }
    Ok((out, exact))
}

/// What [`expect_text`] reads: the lines, whether `exact derived` is
/// given, and the lines the runner reads itself, with their numbers.
pub type ExpectText = (BTreeSet<Expect>, bool, Vec<(usize, String)>);

/// The lines of an `expect.txt` text as [`expect`] and [`exact_derived`]
/// read them, but for the lines `own` accepts, which a runner reads
/// itself (WP-103: the `use` lines of `tests/rules.rs`) and which are
/// returned, trimmed, with their numbers.
pub fn expect_text(text: &str, own: impl Fn(&str) -> bool) -> Result<ExpectText, String> {
    let mut out = BTreeSet::new();
    let mut exact = false;
    let mut mine = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if own(line) {
            mine.push((i + 1, line.to_owned()));
            continue;
        }
        if line == EXACT_DERIVED {
            if exact {
                return Err(format!("expect.txt:{}: a second `{EXACT_DERIVED}`", i + 1));
            }
            exact = true;
            continue;
        }
        let (word, name) =
            line.split_once(' ').ok_or_else(|| format!("expect.txt:{}: no name", i + 1))?;
        let name = name.trim().to_owned();
        let e = match word {
            "changed" => Expect::Changed(name),
            "added" => Expect::Added(name),
            "removed" => Expect::Removed(name),
            "indeterminate" => Expect::Indeterminate(name),
            "derived-changed" => Expect::DerivedChanged(name),
            "derived-added" => Expect::DerivedAdded(name),
            "derived-removed" => Expect::DerivedRemoved(name),
            _ => return Err(format!("expect.txt:{}: unknown word `{word}`", i + 1)),
        };
        if out.contains(&e) {
            return Err(format!("expect.txt:{}: `{}` a second time", i + 1, e.line()));
        }
        out.insert(e);
    }
    Ok((out, exact, mine))
}

/// The lines of `expect.txt` (absent: none); the directive
/// `exact derived` is not one of them ([`exact_derived`]).
pub fn expect(dir: &Path) -> Result<BTreeSet<Expect>, String> {
    read_expect(dir).map(|(lines, _)| lines)
}

/// Whether `expect.txt` has the directive `exact derived` (at most once).
pub fn exact_derived(dir: &Path) -> Result<bool, String> {
    read_expect(dir).map(|(_, exact)| exact)
}

/// The derived units of `d` under `key`.
fn derived_units(d: &Digested, key: &Digest) -> Vec<(Digest, u64)> {
    d.units
        .get(key)
        .into_iter()
        .flatten()
        .filter(|u| u.class == GroupClass::Derived)
        .map(|u| (u.digest, u.flags))
        .collect()
}

/// The derived keys that differ between the sides, each as the line that
/// names it under `exact derived`: `derived-added` (only B has a derived
/// unit under it), `derived-removed` (only A), `derived-changed` (both,
/// and they differ). A key is named by [`Digested::name`], from side A
/// when A has the key; two keys of one name that differ the same way are
/// one line, as in the other lines.
pub fn derived_differences(a: &Digested, b: &Digested) -> BTreeSet<Expect> {
    let keys: BTreeSet<_> = a.units.keys().chain(b.units.keys()).copied().collect();
    let mut out = BTreeSet::new();
    for k in &keys {
        let (da, db) = (derived_units(a, k), derived_units(b, k));
        if da == db {
            continue;
        }
        let name = if a.units.contains_key(k) { a.name(k) } else { b.name(k) };
        out.insert(match (da.is_empty(), db.is_empty()) {
            (true, _) => Expect::DerivedAdded(name),
            (_, true) => Expect::DerivedRemoved(name),
            _ => Expect::DerivedChanged(name),
        });
    }
    out
}

/// The units of a side that are indeterminate for a cause other than
/// `Pending` (`Pending` is tolerated while `Area` exists).
pub fn hard_indeterminate(d: &Digested) -> Vec<String> {
    let mut out = Vec::new();
    for v in d.units.values() {
        for u in v {
            if u.flags & 1 != 0 && !matches!(u.cause, Some(Cause::Pending(_)) | None) {
                out.push(format!(
                    "{} ({})",
                    u.name,
                    u.cause.map(|c| c.to_string()).unwrap_or_default()
                ));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The size of the runners' pool (`spec/landed/harness/WP-047.md`):
/// `SLIP_TEST_THREADS` if set, else the smaller of 4 and the available
/// parallelism.
pub fn pool_size() -> Result<usize, String> {
    pool_size_from(std::env::var_os("SLIP_TEST_THREADS").as_deref())
}

/// [`pool_size`] for the value `var` of `SLIP_TEST_THREADS` (`None`:
/// unset). A value that is not a positive decimal number is an error
/// naming the variable.
pub fn pool_size_from(var: Option<&std::ffi::OsStr>) -> Result<usize, String> {
    let Some(v) = var else {
        return Ok(std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4));
    };
    v.to_str()
        .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("SLIP_TEST_THREADS={}: not a positive number", v.to_string_lossy()))
}

/// The text of a panic's payload.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic whose payload is not text".to_owned())
}

/// The stack of a pool thread, 8 MiB. The parse and the walk have threads
/// of their own (`slip_kiln::snippet`, [`on_walk_stack`]); a pool thread
/// runs the rest of a directory's work, which ran on libtest's test
/// thread before WP-047: the comparison of the sides, `index`, and
/// whatever a caller's closure does (`tests/resolved.rs`,
/// `tests/records.rs` on open branches). libtest gives its threads
/// 2 MiB, or `RUST_MIN_STACK`; 8 MiB, the main thread's usual limit on
/// Linux, keeps that work from depending on either. It is address space,
/// not memory, until it is touched.
const POOL_STACK: usize = 8 << 20;

/// `f` over every item on `threads` threads (at least one), the results
/// in the items' order. A panic in `f` is that item's `Err`, with the
/// panic's text; the other items still run.
pub fn pool_map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<Result<R, String>> {
    pool_map_timed(items, threads, f).into_iter().map(|(r, _)| r).collect()
}

/// [`pool_map`], each result beside the time its item took, a panicking
/// one's included.
pub fn pool_map_timed<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<(Result<R, String>, std::time::Duration)> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        for i in 0..threads.max(1).min(items.len()) {
            let (tx, next, f) = (tx.clone(), &next, &f);
            std::thread::Builder::new()
                .name(format!("pool-{i}"))
                .stack_size(POOL_STACK)
                .spawn_scoped(scope, move || {
                    loop {
                        let n = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(n) else { break };
                        let start = std::time::Instant::now();
                        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(item)))
                            .map_err(|p| panic_text(&*p));
                        if tx.send((n, (r, start.elapsed()))).is_err() {
                            break;
                        }
                    }
                })
                .expect("a thread for the pool");
        }
    });
    drop(tx);
    let mut out: Vec<Option<(Result<R, String>, std::time::Duration)>> =
        items.iter().map(|_| None).collect();
    for (n, r) in rx {
        out[n] = Some(r);
    }
    out.into_iter()
        .map(|r| r.unwrap_or_else(|| unreachable!("every item of the pool has a result")))
        .collect()
}

/// What a run of a pair runner over its directories found, in directory
/// order whatever the pool's size.
#[derive(Debug)]
pub struct Outcome {
    pub root: String,
    pub ran: usize,
    /// The pending directories (relative to `tests/`).
    pub skipped: Vec<String>,
    /// `<dir>: <error>` for each failing directory.
    pub failures: Vec<String>,
    /// `<dir>: <note>` for each note of a passing directory.
    pub notes: Vec<String>,
    /// Each directory run and the time its work took.
    pub times: Vec<(String, std::time::Duration)>,
}

impl Outcome {
    /// What the run prints, then the failure, if any, it panics with:
    /// the notes, the `skipped (pending)` lines and the count line; the
    /// failures joined in order. The times are no part of it.
    pub fn report(&self) -> (String, Option<String>) {
        let mut lines: Vec<String> = self.notes.clone();
        lines.extend(self.skipped.iter().map(|s| format!("skipped (pending): {s}")));
        lines.push(format!(
            "{}: {} run, {} skipped, {} failed",
            self.root,
            self.ran,
            self.skipped.len(),
            self.failures.len()
        ));
        let mut text = lines.join("\n");
        text.push('\n');
        let failure = (!self.failures.is_empty()).then(|| {
            format!(
                "{} of {} failed:\n{}",
                self.failures.len(),
                self.ran,
                self.failures.join("\n\n")
            )
        });
        (text, failure)
    }

    /// The `n` slowest directories, slowest first (ties by directory).
    pub fn slowest(&self, n: usize) -> Vec<(String, std::time::Duration)> {
        let mut t = self.times.clone();
        t.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        t.truncate(n);
        t
    }

    /// Prints the report (and, with `SLIP_DIGEST_TIMES=1`, the twenty
    /// slowest directories) and panics with the failures, if any.
    pub fn finish(&self) {
        let (text, failure) = self.report();
        eprint!("{text}");
        if std::env::var_os("SLIP_DIGEST_TIMES").is_some_and(|v| v == "1") {
            eprintln!("{}: the twenty slowest directories", self.root);
            for (d, t) in self.slowest(20) {
                eprintln!("  {:>7} ms  {d}", t.as_millis());
            }
        }
        if let Some(f) = failure {
            panic!("{f}");
        }
    }
}

/// Runs `f` on each of `dirs` (a runner's directories, in order) on a
/// pool of `threads` threads: the pending ones are skipped and listed, a
/// failure or a panic is that directory's, the notes of a passing one
/// are kept, everything in directory order.
pub fn run_dirs(
    root: &str,
    dirs: &[PathBuf],
    threads: usize,
    f: impl Fn(&Path) -> Result<Vec<String>, String> + Sync,
) -> Outcome {
    let mut out = Outcome {
        root: root.to_owned(),
        ran: 0,
        skipped: Vec::new(),
        failures: Vec::new(),
        notes: Vec::new(),
        times: Vec::new(),
    };
    let mut work = Vec::new();
    for dir in dirs {
        if is_pending(dir) {
            out.skipped.push(rel(dir));
        } else {
            work.push(dir.clone());
        }
    }
    out.ran = work.len();
    let results = pool_map_timed(&work, threads, |dir| f(dir));
    for (dir, (r, t)) in work.iter().zip(results) {
        let rel = rel(dir);
        match r {
            Ok(Ok(notes)) => out.notes.extend(notes.into_iter().map(|n| format!("{rel}: {n}"))),
            Ok(Err(e)) => out.failures.push(format!("{rel}: {e}")),
            Err(panic) => out.failures.push(format!("{rel}: panicked: {panic}")),
        }
        out.times.push((rel, t));
    }
    out
}

/// The directories of `root`, narrowed by `SLIP_DIGEST_ONLY=<text>` to
/// those whose path relative to `tests/` contains the text (it says so).
fn run_dirs_of(root: &str) -> Vec<PathBuf> {
    let mut dirs = dirs(root);
    if let Some(only) = std::env::var_os("SLIP_DIGEST_ONLY") {
        let only = only.to_string_lossy().into_owned();
        let all = dirs.len();
        dirs.retain(|d| rel(d).contains(&only));
        eprintln!("SLIP_DIGEST_ONLY={only}: {} of {all} directories", dirs.len());
    }
    dirs
}

/// [`run_all`] keeping the notes `f` gives a passing directory, and
/// returning the outcome without printing it or failing: the caller
/// ends with [`Outcome::finish`].
pub fn run_collect(root: &str, f: impl Fn(&Path) -> Result<Vec<String>, String> + Sync) -> Outcome {
    let threads = pool_size().unwrap_or_else(|e| panic!("{e}"));
    run_dirs(root, &run_dirs_of(root), threads, f)
}

/// Runs `f` on every directory of `root`, collects every failure with its
/// directory, and panics with all of them; lists the skipped ones. The
/// directories run on a pool of [`pool_size`] threads; what is reported
/// is in directory order whatever the pool (`spec/landed/harness/WP-047.md`).
///
/// With `SLIP_DIGEST_ONLY=<text>` in the environment, only the directories
/// whose path relative to `tests/` contains the text, to look at a few
/// with `SLIP_DIGEST_DUMP`; it says so. Nothing else narrows: the
/// determinism and tables tests use every directory, and `cargo xtask ci`
/// removes the variable from what it runs. With `SLIP_DIGEST_TIMES=1`,
/// the twenty slowest directories and their milliseconds.
pub fn run_all(root: &str, f: impl Fn(&Path) -> Result<(), String> + Sync) {
    run_collect(root, |d| f(d).map(|()| Vec::new())).finish();
}

#[cfg(test)]
mod header_tests {
    //! The header lines of a main file (WP-038, "The runner";
    //! `spec/landed/harness/WP-038.md`).

    use super::{Header, header};

    fn h(std: &str, errors: bool, args: &[&str], target: Option<&str>) -> Header {
        Header {
            std: std.to_owned(),
            errors,
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            target: target.map(str::to_owned),
            modules: Vec::new(),
        }
    }

    // WP-048: `// module: <name>=<file>`, once per module unit, in order;
    // a name or a file twice, a name that is no module name and a value
    // without `=` are errors of the test.
    #[test]
    fn wp048_module_header_lines() {
        let two =
            header("// module: m=m.cppm\n// std: c++23\n// module: m.n:p=part.cppm\nint x;\n")
                .unwrap();
        assert_eq!(
            two.modules,
            [("m".to_owned(), "m.cppm".to_owned()), ("m.n:p".to_owned(), "part.cppm".to_owned())]
        );
        assert_eq!(two.std, "c++23");
        assert_eq!(
            two.line(),
            "-std=c++23 -fmodule-file=m=m.cppm -fmodule-file=m.n:p=part.cppm -fsyntax-only"
        );
        for bad in [
            "// module: m\n",
            "// module: =m.cppm\n",
            "// module: m=\n",
            "// module: 1m=m.cppm\n",
            "// module: m.=m.cppm\n",
            "// module: m:=m.cppm\n",
            "// module: m:p:q=m.cppm\n",
            "// module: m-n=m.cppm\n",
            "// module: m=a b\n",
            "// module: m=a=b\n",
            "// modules: m=m.cppm\n",
        ] {
            assert!(header(bad).is_err(), "{bad:?}");
        }
        let name = header("// module: m=a.cppm\n// module: m=b.cppm\n").unwrap_err();
        assert_eq!(name, "line 2: module `m` is given twice");
        let file = header("// module: m=a.cppm\n// module: n=a.cppm\n").unwrap_err();
        assert_eq!(file, "line 2: file `a.cppm` is given twice");
        assert_eq!(
            header("int x;\n// module: m=m.cppm\n").unwrap_err(),
            "line 2: header line outside the header block"
        );
    }

    #[test]
    fn wp038_header_lines() {
        assert_eq!(header("int x;\n"), Ok(h("c++20", false, &[], None)));
        assert_eq!(
            header("#include \"side.h\"\n// std: c++26\n// §1 a comment\n"),
            Ok(h("c++26", false, &[], None))
        );
        // Each form, in any order, before or after the include.
        let all = "// target: aarch64-unknown-linux-gnu\n// errors: yes\n#include \"side.h\"\n\
                   // args: -fno-rtti  -frounding-math\n// std: c++17\nint x;\n";
        assert_eq!(
            header(all),
            Ok(h(
                "c++17",
                true,
                &["-fno-rtti", "-frounding-math"],
                Some("aarch64-unknown-linux-gnu")
            ))
        );
        // A header line after the end of the block is an error
        // (`wp038_header_line_outside_block`).
        let outside =
            |line: usize| Err(format!("line {line}: header line outside the header block"));
        assert_eq!(header("// a comment\n// args: -fno-rtti\n"), outside(2));
        assert_eq!(header("int x;\n// std: c++17\n"), outside(2));
        // Comments that do not read as `word:` end the block quietly.
        assert_eq!(
            header("// std: c++23\n// WP-028 review, digest §7: x\nint x;\n"),
            Ok(h("c++23", false, &[], None))
        );
        assert_eq!(
            header("// std: c++23\n// WP-028 review, digest §7: x\n// args: -fno-rtti\n"),
            outside(3)
        );
    }

    #[test]
    fn wp038_misspelt_header_lines() {
        for bad in [
            "// arg: -fno-rtti\n",
            "//target: aarch64\n",
            "//  std: c++17\n",
            "// std:c++17\n",
            "// Std: c++17\n",
            "// std : c++17\n",
            "// errors: no\n",
            "// args: \n",
            "// target: a b\n",
            "// Note: a comment that reads like a header line\n",
            "#include \"side.h\"\n// targets: x\n",
        ] {
            assert!(header(bad).is_err(), "{bad:?}");
        }
        let twice = header("// std: c++17\n// std: c++20\n").unwrap_err();
        assert!(twice.contains("a second `// std:` line"), "{twice}");
        let named = header("// std: c++17\n// arg: -x\n").unwrap_err();
        assert!(named.starts_with("line 2: `// arg: -x`"), "{named}");
    }

    // WP-038 review: a header line after the block has ended is an error
    // of the test, not silently unread; a comment like one is not.
    #[test]
    fn wp038_header_line_outside_block() {
        for (text, line) in [
            ("int x;\n// std: c++26\n", 2),
            ("// std: c++17\n\n// args: -fno-rtti\n", 3),
            ("// §1 a comment\n// target: aarch64\n", 2),
            ("\u{feff}// std: c++26\nint x;\n", 1),
            ("/* a */\n// errors: yes\n", 2),
            ("#include \"side.h\"\nint x;\n#ifdef B\n// args: -fno-rtti\n#endif\n", 4),
        ] {
            let err = header(text).unwrap_err();
            assert_eq!(
                err,
                format!("line {line}: header line outside the header block"),
                "{text:?}"
            );
        }
        for text in
            ["int x;\n// errors: no\n", "int x;\n// Note: std: c++26\n", "int x; // std: c++26\n"]
        {
            assert!(header(text).is_ok(), "{text:?}");
        }
    }

    // Both sides of a `t.cpp` test read its lines; each side of an
    // `a.cpp` / `b.cpp` test its own, so a pair may differ in its
    // `// args:` line alone.
    #[test]
    fn wp038_header_lines_by_side() {
        use super::{Side, side_files};
        let files = |list: &[(&str, &str)]| -> std::collections::BTreeMap<String, String> {
            list.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect()
        };
        let t = files(&[("t.cpp", "#include \"side.h\"\n// args: -fno-rtti\nint x;\n")]);
        for side in [Side::A, Side::B] {
            assert_eq!(side_files(&t, side).unwrap().1.args, ["-fno-rtti"]);
        }
        let ab = files(&[
            ("a.cpp", "// args: -frounding-math\ndouble x;\n"),
            ("b.cpp", "double x;\n"),
            ("h.h", "// args: -fno-rtti\n"),
        ]);
        let (list, a) = side_files(&ab, Side::A).unwrap();
        assert_eq!(a.args, ["-frounding-math"]);
        assert!(list.iter().any(|(n, _)| n == "h.h"), "{list:?}");
        assert!(side_files(&ab, Side::B).unwrap().1.args.is_empty());
        let bad = files(&[("a.cpp", "int x;\n"), ("b.cpp", "// arg: -x\n")]);
        assert!(side_files(&bad, Side::A).is_ok());
        assert!(side_files(&bad, Side::B).unwrap_err().starts_with("side B: line 1"));
    }

    #[test]
    fn wp038_header_line_of_side() {
        let all = h("c++23", false, &["-fno-rtti"], Some("aarch64"));
        assert_eq!(all.line(), "-std=c++23 --target=aarch64 -fno-rtti -fsyntax-only");
        assert_eq!(h("c++20", true, &[], None).line(), "-std=c++20 -fsyntax-only");
    }
}
