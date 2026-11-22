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
/// and `_/<slug>`. With `SLIP_DIGEST_ONLY=<text>` in the environment,
/// only those whose path relative to `tests/` contains the text (to look
/// at a few with `SLIP_DIGEST_DUMP`; the tables test does not use this).
pub fn dirs(root: &str) -> Vec<PathBuf> {
    let mut out = all_dirs(root);
    if let Some(only) = std::env::var_os("SLIP_DIGEST_ONLY") {
        let only = only.to_string_lossy().into_owned();
        out.retain(|d| rel(d).contains(&only));
    }
    out
}

fn all_dirs(root: &str) -> Vec<PathBuf> {
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
}

impl Header {
    /// The command line the side is parsed under, as `kiln` would be
    /// given it (for `SLIP_DIGEST_DUMP` and messages).
    pub fn line(&self) -> String {
        let mut line = vec![format!("-std={}", self.std)];
        line.extend(self.target.iter().map(|t| format!("--target={t}")));
        line.extend(self.args.iter().cloned());
        line.push("-fsyntax-only".to_owned());
        line.join(" ")
    }
}

/// The keys of the header lines.
const HEADER_KEYS: [&str; 4] = ["std", "errors", "args", "target"];

/// Whether `line` looks like a header line: `//`, any blanks, a word of
/// ASCII letters and `-`, any blanks, then `:`. A comment that reads so
/// (`// Note: …`) cannot open a main file.
fn looks_like_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("//") else { return false };
    let rest = rest.trim_start();
    let word = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == '-').len();
    word > 0 && rest[..word].starts_with(|c: char| c.is_ascii_alphabetic()) && rest[word..].trim_start().starts_with(':')
}

/// The header lines of a main file: the lines from its top that are
/// header lines or `#include "side.h"` (a `t.cpp` begins with it), up to
/// the first that is neither; a header line after that is not read.
///
/// A header line is exactly `// <key>: <value>` (one space after `//`
/// and after the colon, the value not empty) with the key `std`,
/// `errors` (value `yes`), `args` (split at spaces; no quoting) or
/// `target`, each at most once. A line of the block that looks like a
/// header line ([`looks_like_header`]) but is not one (`// arg: x`,
/// `//target: x`, `// errors: no`, a key twice) is an error of the test.
pub fn header(main: &str) -> Result<Header, String> {
    let mut h = Header { std: "c++20".to_owned(), errors: false, args: Vec::new(), target: None };
    let mut seen: Vec<&str> = Vec::new();
    for (i, line) in main.lines().enumerate() {
        let line = line.trim_end();
        if line == "#include \"side.h\"" {
            continue;
        }
        if !looks_like_header(line) {
            break;
        }
        let bad = || {
            format!(
                "line {}: `{line}` looks like a header line but is not `// std: <std>`, \
                 `// errors: yes`, `// args: <arguments>` or `// target: <triple>`",
                i + 1
            )
        };
        let (key, value) =
            line.strip_prefix("// ").and_then(|r| r.split_once(": ")).ok_or_else(bad)?;
        if !HEADER_KEYS.contains(&key) || value.is_empty() || value.starts_with(' ') {
            return Err(bad());
        }
        if seen.contains(&key) {
            return Err(format!("line {}: a second `// {key}:` line", i + 1));
        }
        seen.push(key);
        match key {
            "std" => value.clone_into(&mut h.std),
            "errors" if value == "yes" => h.errors = true,
            "args" => h.args = value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect(),
            "target" if !value.contains(' ') => h.target = Some(value.to_owned()),
            _ => return Err(bad()),
        }
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
            let options = Options { rules, detail: Detail::All, names: true };
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
    DerivedChanged(String),
}

/// The lines of `expect.txt` (absent: none).
pub fn expect(dir: &Path) -> Result<BTreeSet<Expect>, String> {
    let Ok(text) = std::fs::read_to_string(dir.join("expect.txt")) else {
        return Ok(BTreeSet::new());
    };
    let mut out = BTreeSet::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, name) =
            line.split_once(' ').ok_or_else(|| format!("expect.txt:{}: no name", i + 1))?;
        let name = name.trim().to_owned();
        out.insert(match word {
            "changed" => Expect::Changed(name),
            "added" => Expect::Added(name),
            "removed" => Expect::Removed(name),
            "indeterminate" => Expect::Indeterminate(name),
            "derived-changed" => Expect::DerivedChanged(name),
            _ => return Err(format!("expect.txt:{}: unknown word `{word}`", i + 1)),
        });
    }
    Ok(out)
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

/// Runs `f` on every directory of `root`, collects every failure with its
/// directory, and panics with all of them; lists the skipped ones.
pub fn run_all(root: &str, f: impl Fn(&Path) -> Result<(), String>) {
    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    let mut ran = 0;
    for dir in dirs(root) {
        if is_pending(&dir) {
            skipped.push(rel(&dir));
            continue;
        }
        ran += 1;
        if let Err(e) = f(&dir) {
            failures.push(format!("{}: {e}", rel(&dir)));
        }
    }
    for s in &skipped {
        eprintln!("skipped (pending): {s}");
    }
    eprintln!("{root}: {ran} run, {} skipped, {} failed", skipped.len(), failures.len());
    assert!(failures.is_empty(), "{} of {ran} failed:\n{}", failures.len(), failures.join("\n\n"));
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
        }
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
            Ok(h("c++17", true, &["-fno-rtti", "-frounding-math"], Some("aarch64-unknown-linux-gnu")))
        );
        // A header line after the first line that is not one is not read.
        assert_eq!(
            header("// a comment\n// args: -fno-rtti\n"),
            Ok(h("c++20", false, &[], None))
        );
        assert_eq!(header("int x;\n// std: c++17\n"), Ok(h("c++20", false, &[], None)));
        // Comments that do not read as `word:` end the block quietly.
        assert_eq!(
            header("// std: c++23\n// WP-028 review, digest §7: x\n// args: -fno-rtti\n"),
            Ok(h("c++23", false, &[], None))
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
