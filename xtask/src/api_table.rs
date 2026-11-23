//! `cargo xtask api-table [§N[.M]…]`: the table of rules to script test
//! directories (`docs/wp/WP-091.md`, "Done when"), made from the header
//! lines of each `transform.cpp` under [`ROOT`] and never by hand.
//!
//! A row is one directory: the value of its `// spec:` line (`§6.1 rule 3`),
//! its path under [`ROOT`], and the reason of its `// xfail:` line or
//! nothing. Rows are ordered by that value, numbers compared as numbers
//! (`rule 2` before `rule 10`), then by path. The arguments keep the rows
//! whose section is one of them or lies under one (`§6` keeps `§6.1`, not
//! `§60`); none keeps every row. A last line counts the rows and xfails.
//!
//! A landed file holds a table between `<!-- api-table §4 §5.4 -->` and
//! `<!-- /api-table -->` (the sections are those of the command line); the
//! test [`tests::the_landed_tables_are_current`] checks that every such
//! block is what this command prints for its sections now.

use crate::lint::api_tests::{ROOT, looks_like_header, named_section, spec_line};
use crate::repo::Repo;
use std::cmp::Ordering;
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

/// The opening of a generated table's block: `<!-- api-table` then the
/// sections, then `-->`.
pub const OPEN: &str = "<!-- api-table";

/// The closing line of a generated table's block.
pub const CLOSE: &str = "<!-- /api-table -->";

/// One script test directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The value of the `// spec:` line.
    pub spec: String,
    /// The directory, relative to [`ROOT`].
    pub dir: String,
    /// The reason of the `// xfail:` line.
    pub xfail: Option<String>,
}

/// The value of the `// xfail:` line of a transform's header block.
fn xfail_line(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim_end)
        .take_while(|l| looks_like_header(l))
        .find_map(|l| l.strip_prefix("// xfail:").map(str::trim))
}

/// `value` cut into runs of digits (compared as numbers) and other runs.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn runs(s: &str) -> Vec<(bool, &str)> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = s.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                out.push((bytes[start].is_ascii_digit(), &s[start..i]));
                start = i;
            }
        }
        out
    }
    let (ra, rb) = (runs(a), runs(b));
    for (x, y) in ra.iter().zip(&rb) {
        let o = match (x, y) {
            ((true, x), (true, y)) => {
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                x.len().cmp(&y.len()).then_with(|| x.cmp(y))
            }
            ((_, x), (_, y)) => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ra.len().cmp(&rb.len()).then_with(|| a.cmp(b))
}

/// Whether the section `s` is `filter` or lies under it.
fn under(s: &str, filter: &str) -> bool {
    let filter = filter.strip_prefix('§').unwrap_or(filter);
    s == filter || s.strip_prefix(filter).is_some_and(|r| r.starts_with('.'))
}

/// The rows of `tests` (each a directory under [`ROOT`] and the text of its
/// `transform.cpp`) whose section is under one of `sections` (every row if
/// `sections` is empty), in table order. A directory with no `// spec:`
/// line naming a section is left out; the `api-tests` lint reports it.
pub fn rows<'a>(
    tests: impl IntoIterator<Item = (&'a str, &'a str)>,
    sections: &[String],
) -> Vec<Row> {
    let mut out: Vec<Row> = tests
        .into_iter()
        .filter_map(|(dir, text)| {
            let (_, spec) = spec_line(text)?;
            let section = named_section(spec)?;
            (sections.is_empty() || sections.iter().any(|f| under(section, f))).then(|| Row {
                spec: spec.to_owned(),
                dir: dir.to_owned(),
                xfail: xfail_line(text).map(str::to_owned),
            })
        })
        .collect();
    out.sort_by(|a, b| natural_cmp(&a.spec, &b.spec).then_with(|| natural_cmp(&a.dir, &b.dir)));
    out
}

/// A table cell: `|` escaped, as Markdown needs.
fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// The Markdown table of `rows` and the line that counts them.
pub fn render(rows: &[Row]) -> String {
    let mut out = String::from("| Rule | Directory | xfail |\n|---|---|---|\n");
    for r in rows {
        let _ = writeln!(
            out,
            "| {} | `{}` | {} |",
            cell(&r.spec),
            r.dir,
            r.xfail.as_deref().map_or_else(String::new, cell)
        );
    }
    let xfails = rows.iter().filter(|r| r.xfail.is_some()).count();
    let _ = write!(out, "\n{} directories, {xfails} xfail.\n", rows.len());
    out
}

/// The table of the repository at `root` for `sections`.
pub fn table(repo: &Repo, sections: &[String]) -> String {
    let prefix = format!("{ROOT}/");
    let tests: Vec<(String, String)> = repo
        .files()
        .filter_map(|f| f.strip_prefix(&prefix)?.strip_suffix("/transform.cpp"))
        .map(|d| {
            (d.to_owned(), repo.read(&format!("{ROOT}/{d}/transform.cpp")).unwrap_or_default())
        })
        .collect();
    render(&rows(tests.iter().map(|(d, t)| (d.as_str(), t.as_str())), sections))
}

/// The blocks of `text` between [`OPEN`] and [`CLOSE`]: the sections the
/// opening line names, and the text between the two lines.
pub fn blocks(text: &str) -> Vec<(Vec<String>, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim().strip_prefix(OPEN) else { continue };
        let Some(rest) = rest.strip_suffix("-->") else { continue };
        let sections = rest.split_whitespace().map(str::to_owned).collect();
        let mut body = String::new();
        for l in lines.by_ref() {
            if l.trim() == CLOSE {
                break;
            }
            body.push_str(l);
            body.push('\n');
        }
        out.push((sections, body));
    }
    out
}

/// `cargo xtask api-table`: prints the table.
pub fn main(root: &Path, args: &[String]) -> ExitCode {
    if let Some(bad) = args.iter().find(|a| named_section(a).is_none()) {
        eprintln!(
            "xtask api-table: `{bad}` is not a section (§N or §N.M)\nusage: cargo xtask api-table [§N[.M]…]"
        );
        return ExitCode::from(2);
    }
    match Repo::open(root) {
        Ok(repo) => {
            print!("{}", table(&repo, args));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xtask api-table: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{blocks, natural_cmp, render, rows, table};
    use crate::repo::Repo;
    use std::cmp::Ordering;

    #[test]
    fn numbers_compare_as_numbers() {
        assert_eq!(natural_cmp("§6.1 rule 2", "§6.1 rule 10"), Ordering::Less);
        assert_eq!(natural_cmp("§6.10", "§6.9"), Ordering::Greater);
        assert_eq!(natural_cmp("§4 rule 1", "§4 table x"), Ordering::Less);
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
    }

    #[test]
    fn rows_are_filtered_and_ordered() {
        let tests = [
            ("lookup/r10", "// spec: §6.1 rule 10\n"),
            ("lookup/r2", "// spec: §6.1 rule 2\n// xfail: a | b\n"),
            ("code/x", "// spec: §7.1 rule 1\n"),
            ("odd/sixty", "// spec: §60 rule 1\n"),
            ("none", "#include <x>\n"),
            ("late", "// args: -DX\n\n// xfail: not in the header block\n// spec: §6.1\n"),
        ];
        let got = rows(tests, &["§6".to_owned()]);
        let dirs: Vec<&str> = got.iter().map(|r| r.dir.as_str()).collect();
        assert_eq!(dirs, ["lookup/r2", "lookup/r10"]);
        assert_eq!(got[0].xfail.as_deref(), Some("a | b"));
        assert_eq!(rows(tests, &[]).len(), 4);
        assert_eq!(
            render(&got),
            "| Rule | Directory | xfail |\n|---|---|---|\n\
             | §6.1 rule 2 | `lookup/r2` | a \\| b |\n\
             | §6.1 rule 10 | `lookup/r10` |  |\n\n2 directories, 1 xfail.\n"
        );
    }

    #[test]
    fn blocks_are_found() {
        let text = "x\n<!-- api-table §4 §6.1 -->\n| a |\n<!-- /api-table -->\ny\n";
        assert_eq!(
            blocks(text),
            [(vec!["§4".to_owned(), "§6.1".to_owned()], "| a |\n".to_owned())]
        );
    }

    /// Every generated block in `spec/landed/` is what the command prints
    /// for its sections now.
    #[test]
    fn the_landed_tables_are_current() {
        let repo = Repo::open(&crate::repo_root()).expect("the repository lists its files");
        let landed: Vec<&str> = repo.files().filter(|f| f.starts_with("spec/landed/")).collect();
        for file in landed {
            let text = repo.read(file).unwrap_or_default();
            for (sections, body) in blocks(&text) {
                let want = table(&repo, &sections);
                assert!(
                    body.trim() == want.trim(),
                    "{file}: the table for {sections:?} is stale; regenerate it with \
                     `cargo xtask api-table {}`",
                    sections.join(" ")
                );
            }
        }
    }
}
