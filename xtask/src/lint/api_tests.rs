//! The `api-tests` lint (`docs/wp/WP-090.md`,
//! `spec/landed/api/WP-090.md`): the script test directories of
//! `spec/api.md` §11 under [`ROOT`].
//!
//! A script test directory is one that holds a listed `transform.cpp`.
//! Every such directory also holds `t.cpp` and `expect.txt`; none lies
//! inside another; every listed file under [`ROOT`] lies in one (the
//! files of its subject may be in subdirectories). The `// spec:` header
//! line of each `transform.cpp` (in its header block, the leading lines
//! that look like header lines) names a heading of `spec/api.md`: `§N`
//! for `## N. …`, `§N.M` for `### N.M …`, optionally followed by a space
//! and more (`§5.1 rule 2`).

use super::{Violation, prose_lines};
use crate::repo::Repo;
use std::collections::BTreeSet;

const LINT: &str = "api-tests";

/// The directory of the script tests.
pub const ROOT: &str = "crates/slip_script/tests/api";

/// The spec the `// spec:` lines name.
pub const SPEC: &str = "spec/api.md";

/// The section numbers of the headings of `text`: `## N. Title` gives
/// `N`, `### N.M Title` (any depth of `#` from two, any depth of number)
/// gives `N.M`. Headings in fenced code are not headings.
pub fn section_numbers(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, line) in prose_lines(text) {
        let hashes = line.len() - line.trim_start_matches('#').len();
        if hashes < 2 {
            continue;
        }
        let Some(rest) = line[hashes..].strip_prefix(' ') else { continue };
        let Some((number, _)) = rest.split_once(' ') else { continue };
        let number = number.strip_suffix('.').unwrap_or(number);
        if !number.is_empty()
            && number.split('.').all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
            out.insert(number.to_owned());
        }
    }
    out
}

/// Whether `line` looks like a header line (`spec/harness.md` §6.1): `//`,
/// blanks, a word of ASCII letters and `-` beginning with a letter,
/// blanks, `:`.
pub(crate) fn looks_like_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("//") else { return false };
    let rest = rest.trim_start();
    let word =
        rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == '-').len();
    word > 0
        && rest[..word].starts_with(|c: char| c.is_ascii_alphabetic())
        && rest[word..].trim_start().starts_with(':')
}

/// The `// spec:` line of a transform's header block: its line number and
/// value.
pub(crate) fn spec_line(text: &str) -> Option<(usize, &str)> {
    text.lines()
        .map(str::trim_end)
        .enumerate()
        .take_while(|(_, l)| looks_like_header(l))
        .find_map(|(i, l)| l.strip_prefix("// spec: ").map(|v| (i + 1, v)))
}

/// The section a `// spec:` value names: the number after `§`, before a
/// space.
pub(crate) fn named_section(value: &str) -> Option<&str> {
    let rest = value.strip_prefix('§')?;
    let number = rest.split_once(' ').map_or(rest, |(n, _)| n);
    (!number.is_empty()
        && number.split('.').all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())))
    .then_some(number)
}

/// `api-tests`.
pub fn check(repo: &Repo) -> Vec<Violation> {
    let prefix = format!("{ROOT}/");
    let under: Vec<&str> = repo.files().filter(|f| f.starts_with(&prefix)).collect();
    let tests: Vec<&str> = under.iter().filter_map(|f| f.strip_suffix("/transform.cpp")).collect();
    let mut out = Vec::new();
    let inside = |f: &str, dir: &str| f.starts_with(&format!("{dir}/"));
    for &f in &under {
        if !tests.iter().any(|d| inside(f, d)) {
            out.push(Violation::new(
                f,
                None,
                LINT,
                "is in no script test directory (one with a transform.cpp)",
            ));
        }
    }
    let sections = repo.read(SPEC).map(|t| section_numbers(&t));
    for &dir in &tests {
        if let Some(outer) = tests.iter().find(|&&o| o != dir && inside(dir, o)) {
            out.push(Violation::new(
                format!("{dir}/transform.cpp"),
                None,
                LINT,
                format!("a script test inside the directory of the script test {outer}"),
            ));
        }
        for name in ["t.cpp", "expect.txt"] {
            if !repo.is_file(&format!("{dir}/{name}")) {
                out.push(Violation::new(dir, None, LINT, format!("has no {name}")));
            }
        }
        let file = format!("{dir}/transform.cpp");
        let text = repo.read(&file).unwrap_or_default();
        let Some((line, value)) = spec_line(&text) else {
            out.push(Violation::new(&file, None, LINT, "has no `// spec:` header line"));
            continue;
        };
        let Some(section) = named_section(value) else {
            out.push(Violation::new(
                &file,
                Some(line),
                LINT,
                format!("`// spec: {value}` does not name a section as `§N` or `§N.M`"),
            ));
            continue;
        };
        match &sections {
            Some(s) if s.contains(section) => {}
            Some(_) => out.push(Violation::new(
                &file,
                Some(line),
                LINT,
                format!("`// spec: {value}`: {SPEC} has no heading §{section}"),
            )),
            None => out.push(Violation::new(
                &file,
                Some(line),
                LINT,
                format!("`// spec: {value}`: {SPEC} cannot be read"),
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{named_section, section_numbers, spec_line};

    #[test]
    fn headings_give_their_numbers() {
        let text = "# api\n## 2. The universe\n### 2.1 The unit\n#### 2.1.3 Deep\n\
                    ## Tests\n```\n## 9. Fenced\n```\n##3. No space\n";
        let got: Vec<String> = section_numbers(text).into_iter().collect();
        assert_eq!(got, ["2", "2.1", "2.1.3"]);
    }

    #[test]
    fn the_spec_line_is_read_from_the_header_block_only() {
        assert_eq!(spec_line("// args: -DX\n// spec: §2.1\n#include <x>\n"), Some((2, "§2.1")));
        assert_eq!(spec_line("#include <x>\n// spec: §2.1\n"), None);
        assert_eq!(spec_line("// spec:§2.1\n"), None);
        assert_eq!(named_section("§5.1 rule 2"), Some("5.1"));
        assert_eq!(named_section("5.1"), None);
        assert_eq!(named_section("§5..1"), None);
        assert_eq!(named_section("§"), None);
    }
}
