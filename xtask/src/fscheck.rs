//! `cargo xtask fscheck <dir> [--rounds <n>] [--row <name>]…`: the probe of
//! a file system that confuses the names of a hard-linked file
//! (`spec/harness.md` §11).
//!
//! Each row repeats, `rounds` times, one sequence of create, link, unlink
//! and rename in a directory of its own under `<dir>`, then reads the
//! names the sequence must have left with known contents. A read that
//! returns other bytes, or fails, is a **wrong read**; so is a step of the
//! sequence that fails (a `create_new` of a name just unlinked that finds
//! it still there, `EEXIST`): the round is counted, its names removed at
//! the start of the next, and the row goes on. A row the harness or
//! the store relies on must have none; the others are the patterns the
//! harness and the store do not use, run to show why.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

/// A row of the probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub name: &'static str,
    /// The harness or the store relies on the row having no wrong read.
    pub relied: bool,
    /// What does the sequence, in a few words.
    pub what: &'static str,
}

/// The rows, in the order they run (`spec/harness.md` §11.2).
pub const ROWS: &[Row] = &[
    Row {
        name: "link-unlink-create",
        relied: false,
        what: "B a hard link to A; B unlinked and created again",
    },
    Row {
        name: "link-rename-over",
        relied: false,
        what: "B a hard link to A; a temporary renamed over B",
    },
    Row {
        name: "copy-unlink-create",
        relied: true,
        what: "B a copy of A; B unlinked and created again",
    },
    Row {
        name: "copy-rename-over",
        relied: true,
        what: "B a copy of A; a temporary renamed over B",
    },
    Row {
        name: "fresh-beside-links",
        relied: true,
        what: "a tree of links with one file created fresh in it",
    },
    Row { name: "link-read-both", relied: true, what: "B a hard link to A, read through both" },
    Row {
        name: "tmp-again-after-link",
        relied: false,
        what: "T linked to L and unlinked; T created again",
    },
    Row { name: "tmp-again-after-rename", relied: true, what: "T renamed to D; T created again" },
    Row {
        name: "unique-tmp-link",
        relied: true,
        what: "T linked to L and unlinked; T's name never used again",
    },
];

/// What one row gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub row: Row,
    pub rounds: u64,
    pub wrong: u64,
    /// The first wrong read: the name, what was expected, what was read.
    pub first: Option<String>,
    pub seconds: f64,
}

/// How a name is read: `fs::read`, or a test's reader that plants a
/// difference.
pub type Reader<'a> = &'a dyn Fn(&Path) -> io::Result<Vec<u8>>;

/// The parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub dir: PathBuf,
    pub rounds: u64,
    pub rows: Vec<Row>,
}

/// The rounds of a row when `--rounds` is not given.
pub const DEFAULT_ROUNDS: u64 = 20_000;

const USAGE: &str = "usage: cargo xtask fscheck <dir> [--rounds <n>] [--row <name>]…\n\
rows: link-unlink-create link-rename-over copy-unlink-create copy-rename-over\n      \
fresh-beside-links link-read-both tmp-again-after-link tmp-again-after-rename\n      \
unique-tmp-link";

/// Parses the arguments after `fscheck`; `Err` is a usage error's message.
pub fn parse(args: &[String]) -> Result<Request, String> {
    let mut dir = None;
    let mut rounds = None;
    let mut rows: Vec<Row> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--rounds" => {
                let v = it.next().ok_or("--rounds needs a value")?;
                if rounds.is_some() {
                    return Err("--rounds given twice".into());
                }
                let n: u64 = v
                    .parse()
                    .ok()
                    .filter(|n| *n > 0 && v.bytes().all(|b| b.is_ascii_digit()))
                    .ok_or_else(|| format!("--rounds: `{v}` is not a positive integer"))?;
                rounds = Some(n);
            }
            "--row" => {
                let v = it.next().ok_or("--row needs a value")?;
                let row =
                    ROWS.iter().find(|r| r.name == v).ok_or_else(|| format!("no row `{v}`"))?;
                if !rows.contains(row) {
                    rows.push(*row);
                }
            }
            s if s.starts_with('-') => return Err(format!("unknown option `{s}`")),
            s => {
                if dir.is_some() {
                    return Err(format!("unexpected argument `{s}`"));
                }
                if s.is_empty() {
                    return Err("the directory is empty".into());
                }
                dir = Some(PathBuf::from(s));
            }
        }
    }
    let dir = dir.ok_or("no directory")?;
    // In the order of ROWS, whatever the order of the options.
    let rows = if rows.is_empty() {
        ROWS.to_vec()
    } else {
        ROWS.iter().filter(|r| rows.contains(r)).copied().collect()
    };
    Ok(Request { dir, rounds: rounds.unwrap_or(DEFAULT_ROUNDS), rows })
}

/// `cargo xtask fscheck …`: 0 when every row run that is relied on has no
/// wrong read, 1 when one has (or on an I/O error), 2 on a usage error.
pub fn main(args: &[String]) -> ExitCode {
    let request = match parse(args) {
        Ok(r) => r,
        Err(m) => {
            eprintln!("xtask fscheck: {m}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut out = io::stdout();
    let code = run(&request, &|p| fs::read(p), &mut |line| {
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    });
    ExitCode::from(code)
}

/// Runs the request's rows, printing through `print` as each ends, and
/// returns the exit status.
pub fn run(request: &Request, read: Reader<'_>, print: &mut dyn FnMut(&str)) -> u8 {
    match fs::metadata(&request.dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            print(&format!("fscheck: {}: not a directory", request.dir.display()));
            return 1;
        }
        Err(e) => {
            print(&format!("fscheck: {}: {e}", request.dir.display()));
            return 1;
        }
    }
    print(&format!("fscheck: {}: {} rounds per row", request.dir.display(), request.rounds));
    let mut failed = Vec::new();
    for row in &request.rows {
        match run_row(&request.dir, *row, request.rounds, read) {
            Ok(o) => {
                print(&line(&o));
                if let Some(first) = &o.first {
                    print(&format!("  first: {first}"));
                }
                if o.row.relied && o.wrong > 0 {
                    failed.push(o.row.name);
                }
            }
            Err(e) => {
                print(&format!("{:<24}error: {e}", row.name));
                failed.push(row.name);
            }
        }
    }
    if failed.is_empty() {
        print("fscheck: ok");
        0
    } else {
        print(&format!("fscheck: FAILED ({})", failed.join(", ")));
        1
    }
}

/// `<row> rounds <n> wrong <w> <seconds>s relied on|not relied on: <what>`.
pub fn line(o: &Outcome) -> String {
    format!(
        "{:<24}rounds {} wrong {} {:.1}s  {}: {}",
        o.row.name,
        o.rounds,
        o.wrong,
        o.seconds,
        if o.row.relied { "relied on" } else { "not relied on" },
        o.row.what
    )
}

/// Runs one row in a new directory `<dir>/fscheck.<pid>.<row>`, removed at
/// the end. A step of a round that fails is a wrong read of that round;
/// only failing to make or remove the row's directory is `Err`.
pub fn run_row(dir: &Path, row: Row, rounds: u64, read: Reader<'_>) -> Result<Outcome, String> {
    let base = dir.join(format!("fscheck.{}.{}", std::process::id(), row.name));
    if fs::symlink_metadata(&base).is_ok() {
        fs::remove_dir_all(&base).map_err(|e| at(&base, e))?;
    }
    for sub in ["src", "tree", "tmp"] {
        fs::create_dir_all(base.join(sub)).map_err(|e| at(&base, e))?;
    }
    let mut probe = Probe { read, wrong: 0, first: None };
    let t0 = Instant::now();
    for i in 0..rounds {
        if let Err(e) = round(&base, row.name, i, &mut probe) {
            probe.wrong += 1;
            if probe.first.is_none() {
                probe.first = Some(format!("round {i}: a step failed: {e}"));
            }
        }
    }
    let seconds = t0.elapsed().as_secs_f64();
    fs::remove_dir_all(&base).map_err(|e| at(&base, e))?;
    Ok(Outcome { row, rounds, wrong: probe.wrong, first: probe.first, seconds })
}

struct Probe<'a> {
    read: Reader<'a>,
    wrong: u64,
    first: Option<String>,
}

impl Probe<'_> {
    /// Reads `path` and counts a wrong read if it is not `want`.
    fn check(&mut self, path: &Path, want: &[u8]) {
        let got = (self.read)(path);
        if got.as_deref().ok() == Some(want) {
            return;
        }
        self.wrong += 1;
        if self.first.is_none() {
            let shown = match got {
                Ok(b) => format!("`{}`", String::from_utf8_lossy(&b).trim_end()),
                Err(e) => format!("an error: {e}"),
            };
            self.first = Some(format!(
                "{} read {shown}, not `{}`",
                path.display(),
                String::from_utf8_lossy(want).trim_end()
            ));
        }
    }
}

fn at(path: &Path, e: io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Removes `path` if it exists.
fn unlink(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(at(path, e)),
    }
}

/// Creates `path`, which must not exist, with `bytes`.
fn create(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f =
        fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| at(path, e))?;
    f.write_all(bytes).map_err(|e| at(path, e))
}

fn link(from: &Path, to: &Path) -> Result<(), String> {
    fs::hard_link(from, to).map_err(|e| format!("link {} to {}: {e}", from.display(), to.display()))
}

fn rename(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| format!("rename {} to {}: {e}", from.display(), to.display()))
}

/// One round of `row`. Names are reused every seven rounds, as a tree that
/// is removed and made again reuses its names; each round's contents are
/// its own.
fn round(base: &Path, row: &str, i: u64, p: &mut Probe<'_>) -> Result<(), String> {
    let k = i % 7;
    let (src, tree, tmp) = (base.join("src"), base.join("tree"), base.join("tmp"));
    let a = src.join(format!("a{k}.cpp"));
    let b = tree.join(format!("a{k}.cpp"));
    let pre = format!("pre {i} detail\n").into_bytes();
    let post = format!("post {i} impl\n").into_bytes();
    // A then B (or T then L) start absent.
    let start = |names: &[&Path]| names.iter().try_for_each(|n| unlink(n));
    // The subject is written and read once, as a map run reads it.
    let subject = |p: &mut Probe<'_>| -> Result<(), String> {
        create(&a, &pre)?;
        p.check(&a, &pre);
        Ok(())
    };
    match row {
        "link-unlink-create" | "copy-unlink-create" => {
            start(&[&a, &b])?;
            subject(p)?;
            if row.starts_with("link") {
                link(&a, &b)?;
            } else {
                fs::copy(&a, &b).map_err(|e| at(&b, e))?;
            }
            unlink(&b)?;
            create(&b, &post)?;
            p.check(&b, &post);
            p.check(&a, &pre);
        }
        "link-rename-over" | "copy-rename-over" => {
            start(&[&a, &b])?;
            subject(p)?;
            if row.starts_with("link") {
                link(&a, &b)?;
            } else {
                fs::copy(&a, &b).map_err(|e| at(&b, e))?;
            }
            let t = tree.join(format!("a{k}.cpp.new"));
            create(&t, &post)?;
            rename(&t, &b)?;
            p.check(&b, &post);
            p.check(&a, &pre);
        }
        "fresh-beside-links" => {
            // The subject has a file the edit leaves (`s`) and one it
            // edits (`a`); the tree links `s` and writes `a` fresh.
            let s = src.join(format!("s{k}.h"));
            let ts = tree.join(format!("s{k}.h"));
            let same = format!("same {i}\n").into_bytes();
            start(&[&a, &b, &s, &ts])?;
            subject(p)?;
            create(&s, &same)?;
            p.check(&s, &same);
            link(&s, &ts)?;
            create(&b, &post)?;
            p.check(&b, &post);
            p.check(&a, &pre);
            p.check(&ts, &same);
            p.check(&s, &same);
        }
        "link-read-both" => {
            start(&[&a, &b])?;
            subject(p)?;
            link(&a, &b)?;
            p.check(&b, &pre);
            p.check(&a, &pre);
            p.check(&b, &pre);
        }
        "tmp-again-after-link" => {
            // The lock or the journal of a store, published from a
            // temporary whose name a later process (with the same pid)
            // makes again.
            let t = tmp.join(format!("{k}.0.tmp"));
            let l = tree.join(format!("lock{k}"));
            start(&[&t, &l])?;
            create(&t, &pre)?;
            link(&t, &l)?;
            unlink(&t)?;
            create(&t, &post)?;
            p.check(&l, &pre);
            p.check(&t, &post);
        }
        "tmp-again-after-rename" => {
            // A store's publish (or a report's, or `apply`'s patch): the
            // temporary renamed to its final name, the temporary's name
            // made again by a later process with the same pid.
            let t = tmp.join(format!("{k}.0.tmp"));
            let d = tree.join(format!("entry{k}"));
            start(&[&t, &d])?;
            create(&t, &pre)?;
            rename(&t, &d)?;
            p.check(&d, &pre);
            create(&t, &post)?;
            p.check(&d, &pre);
            p.check(&t, &post);
        }
        "unique-tmp-link" => {
            let t = tmp.join(format!("{i}.tmp"));
            let l = tree.join(format!("lock{k}"));
            start(&[&l])?;
            create(&t, &pre)?;
            link(&t, &l)?;
            unlink(&t)?;
            p.check(&l, &pre);
        }
        other => unreachable!("no row `{other}`"),
    }
    Ok(())
}
