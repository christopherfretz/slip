//! What a configured cmake build tree ("Unix Makefiles" generator) says
//! about itself: the number of entries of its compilation database, and
//! the generated files a set of build targets promise
//! (`spec/landed/harness/WP-011.md` "The generated check").
//!
//! cmake writes one rule file per target, `<dir>/cmakeFiles/<t>.dir/build.make`,
//! where `<t>: <file>` lines list what building the target makes, and
//! `cmakeFiles/Makefile2`, which maps a target name to that directory
//! (`<t>: <dir>/rule`) and records the dependencies between targets
//! (`<dir>/all: <other dir>/all`). Paths there are relative to the build
//! directory unless absolute.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The extensions of a generated file a translation unit can include.
pub const GENERATED: &[&str] = &["inc", "def", "h", "hpp", "hh", "hxx", "gen"];

/// The number of entries (objects of the top-level array) of the
/// compilation database `text`; `Err` if it is not a JSON array of objects.
pub fn database_entries(text: &str) -> Result<usize, String> {
    let mut depth = 0usize;
    let mut entries = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut seen_top = false;
    let mut closed = false;
    for c in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if closed {
            if !c.is_whitespace() {
                return Err("text after the top-level array".into());
            }
            continue;
        }
        match c {
            c if c.is_whitespace() => {}
            '[' if !seen_top => {
                seen_top = true;
                depth = 1;
            }
            _ if !seen_top => return Err("not a JSON array".into()),
            '"' => {
                if depth == 1 {
                    return Err("an entry is not an object".into());
                }
                in_string = true;
            }
            '{' | '[' => {
                if depth == 1 {
                    if c == '[' {
                        return Err("an entry is not an object".into());
                    }
                    entries += 1;
                }
                depth += 1;
            }
            '}' | ']' => {
                depth = depth.checked_sub(1).ok_or("unbalanced brackets")?;
                if depth == 0 {
                    if c != ']' {
                        return Err("unbalanced brackets".into());
                    }
                    closed = true;
                }
            }
            ',' | ':' => {}
            _ if depth == 1 => return Err("an entry is not an object".into()),
            _ => {}
        }
    }
    if !closed {
        return Err("the top-level array is not closed".into());
    }
    Ok(entries)
}

/// The targets of a configured build tree.
#[derive(Debug, Clone, Default)]
pub struct Targets {
    /// Target name to its rule directory (`sub/cmakeFiles/gen.dir`).
    pub dirs: BTreeMap<String, String>,
    /// Rule directory to the rule directories it depends on.
    pub deps: BTreeMap<String, BTreeSet<String>>,
}

/// Reads `cmakeFiles/Makefile2` of the build tree `build`.
pub fn targets(build: &Path) -> Result<Targets, String> {
    let path = build.join("CMakeFiles/Makefile2");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut t = Targets::default();
    for line in rule_lines(&text) {
        let Some((lhs, rhs)) = line.split_once(": ") else { continue };
        let (lhs, rhs) = (lhs.trim(), rhs.trim());
        if let Some(dir) = rhs.strip_suffix("/rule")
            && is_rule_dir(dir)
            && !lhs.contains('/')
        {
            t.dirs.insert(lhs.to_owned(), dir.to_owned());
        } else if let (Some(a), Some(b)) = (lhs.strip_suffix("/all"), rhs.strip_suffix("/all"))
            && is_rule_dir(a)
            && is_rule_dir(b)
        {
            t.deps.entry(a.to_owned()).or_default().insert(b.to_owned());
        }
    }
    Ok(t)
}

/// How to build a list of `generate` entries, and what they promise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The plain target names, built by one `make <targets…>`.
    pub targets: Vec<String>,
    /// The `<target>:<file>` entries as (the target's `build.make`
    /// relative to the build tree, the file), each built by
    /// `make -f <build.make> <file>`.
    pub files: Vec<(String, String)>,
    /// The generated files (by [`GENERATED`] extension) the entries
    /// promise, as absolute paths.
    pub promised: BTreeSet<PathBuf>,
}

/// The plan for `entries` in the build tree `build`. A target promises
/// the outputs listed in the `build.make` of the target and of every
/// target it depends on; a `<target>:<file>` entry promises the file,
/// which must have a rule in the target's `build.make`. `Err` names a
/// target the build tree does not have, or a file without a rule.
pub fn plan(build: &Path, entries: &[String]) -> Result<Plan, String> {
    let t = targets(build)?;
    let by_dir: BTreeMap<&str, &str> =
        t.dirs.iter().map(|(n, d)| (d.as_str(), n.as_str())).collect();
    let dir_of = |n: &str| {
        t.dirs.get(n).ok_or_else(|| {
            format!(
                "target `{n}` is not in the configured build ({})",
                build.join("CMakeFiles/Makefile2").display()
            )
        })
    };
    let read = |dir: &str| {
        let path = build.join(dir).join("build.make");
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
    };
    let mut plan = Plan::default();
    let mut todo: Vec<&str> = Vec::new();
    for e in entries {
        if let Some((n, file)) = e.split_once(':') {
            let dir = dir_of(n)?;
            let has_rule = rule_lines(&read(dir)?)
                .any(|l| l.strip_prefix(file).is_some_and(|r| r.starts_with(':')));
            if !has_rule {
                return Err(format!("`{file}` has no rule in {dir}/build.make (`{e}`)"));
            }
            plan.files.push((format!("{dir}/build.make"), file.to_owned()));
            plan.promised.insert(build.join(file));
        } else {
            todo.push(dir_of(e)?);
            plan.targets.push(e.clone());
        }
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    while let Some(dir) = todo.pop() {
        if !seen.insert(dir) {
            continue;
        }
        if let Some(d) = t.deps.get(dir) {
            todo.extend(d.iter().map(String::as_str));
        }
        let Some(name) = by_dir.get(dir) else { continue };
        for f in outputs(&read(dir)?, name) {
            plan.promised.insert(if Path::new(&f).is_absolute() {
                PathBuf::from(f)
            } else {
                build.join(f)
            });
        }
    }
    Ok(plan)
}

/// The generated files target `name`'s `build.make` (text `text`) lists
/// as made by the target.
fn outputs(text: &str, name: &str) -> Vec<String> {
    let prefix = format!("{name}: ");
    let mut out = Vec::new();
    for line in rule_lines(text) {
        let Some(rest) = line.strip_prefix(&prefix) else { continue };
        for f in rest.split_whitespace() {
            let in_cmake_files = f.split('/').any(|c| c == "CMakeFiles");
            let ext = Path::new(&f).extension().and_then(|e| e.to_str()).unwrap_or_default();
            if !in_cmake_files && GENERATED.contains(&ext) {
                out.push(f.to_owned());
            }
        }
    }
    out
}

/// Whether `s` is a target's rule directory (`…/CMakeFiles/<t>.dir`).
fn is_rule_dir(s: &str) -> bool {
    Path::new(s).extension().is_some_and(|e| e == "dir")
}

/// The lines of a makefile that are rules or assignments: not recipes
/// (tab-indented), comments or blank.
fn rule_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|l| !l.starts_with('\t') && !l.starts_with('#') && !l.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_database_entries() {
        assert_eq!(database_entries("[]").unwrap(), 0);
        let db = r#"[ {"file": "a{[\"", "arguments": ["x"]}, {"file": "b"} ]"#;
        assert_eq!(database_entries(db).unwrap(), 2);
        for bad in ["", "{}", "[1]", "[[]]", "[{}", "[{}] x", r#"["a"]"#] {
            assert!(database_entries(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn outputs_of_a_target() {
        let text = "gen: sub/CMakeFiles/gen\ngen: sub/gen.inc\ngen: sub/CMakeFiles/gen.dir/build.make\n\
                    \tgen: x.inc\n";
        assert_eq!(outputs(text, "gen"), ["sub/gen.inc"]);
    }
}
