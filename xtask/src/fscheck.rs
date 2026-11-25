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
//!
//! The row `create-new-xproc` runs a second process, this program's
//! `fscheck-reader`: this process creates a claim and a file (synced, every
//! eighth round 256 KiB, as a journal; else 32 bytes, as a lock) and asks
//! the reader, over a pipe, to list the claim and read the file whole; then
//! it removes both and asks the reader not to list the claim. A reply other
//! than `ok` is its wrong read (`spec/exec.md` §2.1 "The lock").
//!
//! The row `inode-stable` is of another kind (§11.5): it creates files,
//! reads their device and inode numbers, waits and reads them again. A
//! file whose numbers changed is its **wrong read**: the front end
//! identifies a file by them, so an overlay entry keyed at one moment
//! misses the file loaded at the next.

use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

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
        relied: false,
        what: "T linked to L and unlinked; T's name never used again",
    },
    Row {
        name: "create-new-read",
        relied: true,
        what: "L created exclusively, written, synced, read; a claim listed and removed",
    },
    Row {
        name: "create-new-xproc",
        relied: true,
        what: "a claim and a file (256 KiB every 8th round) created and synced; \
               another process lists the claim and reads the file whole",
    },
    Row {
        name: "inode-stable",
        relied: true,
        what: "a file created fresh keeps its device and inode numbers for 2 s",
    },
];

/// The size of the file of `create-new-xproc` every eighth round: a
/// journal's of a large write.
pub const XPROC_BIG: usize = 256 * 1024;

/// Its size in the other rounds: a lock's.
pub const XPROC_SMALL: usize = 32;

/// The files of `inode-stable` created before its wait.
pub const INODE_BATCH: u64 = 1_000;

/// How long `inode-stable` waits after a batch before it reads the
/// numbers again: the probes of WP-162 saw them change within 2 s.
pub const INODE_WAIT: Duration = Duration::from_secs(2);

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

/// How a name's device and inode numbers are read: [`identity`], or a
/// test's reader that plants a change.
pub type Identity<'a> = &'a dyn Fn(&Path) -> io::Result<(u64, u64)>;

/// The device and inode numbers of the file `path` names (`stat`, as the
/// front end's `SourceManager` identifies a file).
pub fn identity(path: &Path) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).map(|m| (m.dev(), m.ino()))
}

/// The parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub dir: PathBuf,
    pub rounds: u64,
    pub rows: Vec<Row>,
    /// The other process of `create-new-xproc`: its program and leading
    /// arguments (the directory is appended). `None`: this program's
    /// `fscheck-reader` (its name as started).
    pub reader: Option<Vec<OsString>>,
}

/// The rounds of a row when `--rounds` is not given.
pub const DEFAULT_ROUNDS: u64 = 20_000;

const USAGE: &str = "usage: cargo xtask fscheck <dir> [--rounds <n>] [--row <name>]…\n\
rows: link-unlink-create link-rename-over copy-unlink-create copy-rename-over\n      \
fresh-beside-links link-read-both tmp-again-after-link tmp-again-after-rename\n      \
unique-tmp-link create-new-read create-new-xproc inode-stable";

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
    Ok(Request { dir, rounds: rounds.unwrap_or(DEFAULT_ROUNDS), rows, reader: None })
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
    run_with(request, read, &identity, print)
}

/// [`run`] with the numbers of a file read by `ident`.
pub fn run_with(
    request: &Request,
    read: Reader<'_>,
    ident: Identity<'_>,
    print: &mut dyn FnMut(&str),
) -> u8 {
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
        let reader = request.reader.clone().unwrap_or_else(own_reader);
        match run_row_in(&request.dir, *row, request.rounds, (read, ident), &reader) {
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
    run_row_with(dir, row, rounds, read, &identity)
}

/// [`run_row`] with the numbers of a file read by `ident`.
pub fn run_row_with(
    dir: &Path,
    row: Row,
    rounds: u64,
    read: Reader<'_>,
    ident: Identity<'_>,
) -> Result<Outcome, String> {
    run_row_in(dir, row, rounds, (read, ident), &own_reader())
}

/// `fscheck-reader` of this program, by its name as started (on the
/// workspace mount `/proc/self/exe` can read `… (deleted)`).
fn own_reader() -> Vec<OsString> {
    let exe = std::env::args_os()
        .next()
        .or_else(|| std::env::current_exe().ok().map(Into::into))
        .unwrap_or_else(|| "xtask".into());
    vec![exe, "fscheck-reader".into()]
}

fn run_row_in(
    dir: &Path,
    row: Row,
    rounds: u64,
    (read, ident): (Reader<'_>, Identity<'_>),
    reader: &[OsString],
) -> Result<Outcome, String> {
    let base = dir.join(format!("fscheck.{}.{}", std::process::id(), row.name));
    if fs::symlink_metadata(&base).is_ok() {
        fs::remove_dir_all(&base).map_err(|e| at(&base, &e))?;
    }
    for sub in ["src", "tree", "tmp"] {
        fs::create_dir_all(base.join(sub)).map_err(|e| at(&base, &e))?;
    }
    let mut probe = Probe { read, wrong: 0, first: None };
    let t0 = Instant::now();
    if row.name == "create-new-xproc" {
        let mut other = match Other::start(reader, &base.join("tmp")) {
            Ok(o) => o,
            Err(e) => {
                let _ = fs::remove_dir_all(&base);
                return Err(e);
            }
        };
        for i in 0..rounds {
            if let Err(e) = xproc_round(&base.join("tmp"), i, &mut other, &mut probe) {
                probe.fail(format!("round {i}: a step failed: {e}"));
            }
        }
        if let Err(e) = other.end() {
            probe.fail(e);
        }
    } else if row.name == "inode-stable" {
        let mut i = 0;
        while i < rounds {
            let n = INODE_BATCH.min(rounds - i);
            if let Err(e) = inode_batch(&base, i, n, ident, &mut probe) {
                probe.wrong += 1;
                if probe.first.is_none() {
                    probe.first = Some(format!("round {i}: a step failed: {e}"));
                }
            }
            i += n;
        }
    } else {
        for i in 0..rounds {
            if let Err(e) = round(&base, row.name, i, &mut probe) {
                probe.wrong += 1;
                if probe.first.is_none() {
                    probe.first = Some(format!("round {i}: a step failed: {e}"));
                }
            }
        }
    }
    let seconds = t0.elapsed().as_secs_f64();
    fs::remove_dir_all(&base).map_err(|e| at(&base, &e))?;
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

impl Probe<'_> {
    /// Counts a wrong read, `what` if it is the first.
    fn fail(&mut self, what: String) {
        self.wrong += 1;
        if self.first.is_none() {
            self.first = Some(what);
        }
    }

    /// Lists `dir` and counts a wrong read if `name` is in it and should
    /// not be, or is not and should.
    fn listed(&mut self, dir: &Path, name: &str, want: bool) {
        let got = fs::read_dir(dir).map(|entries| {
            entries.filter_map(Result::ok).any(|e| e.file_name().to_str() == Some(name))
        });
        if got.as_ref().ok() == Some(&want) {
            return;
        }
        self.wrong += 1;
        if self.first.is_none() {
            let shown = match got {
                Ok(true) => "lists".to_owned(),
                Ok(false) => "does not list".to_owned(),
                Err(e) => format!("could not be listed ({e}); it should list"),
            };
            let should = if want { "it should" } else { "it should not" };
            self.first = Some(format!("{} {shown} {name}; {should}", dir.display()));
        }
    }
}

fn at(path: &Path, e: &io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Removes `path` if it exists.
fn unlink(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(at(path, &e)),
    }
}

/// Creates `path`, which must not exist, with `bytes`.
fn create(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f =
        fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| at(path, &e))?;
    f.write_all(bytes).map_err(|e| at(path, &e))
}

/// Creates `path`, which must not exist, with `bytes`, and syncs it (the
/// store's lock and journal, `spec/exec.md` §2.1).
fn create_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f =
        fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| at(path, &e))?;
    f.write_all(bytes).and_then(|()| f.sync_all()).map_err(|e| at(path, &e))
}

fn link(from: &Path, to: &Path) -> Result<(), String> {
    fs::hard_link(from, to).map_err(|e| format!("link {} to {}: {e}", from.display(), to.display()))
}

fn rename(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| format!("rename {} to {}: {e}", from.display(), to.display()))
}

/// Rounds `first..first + n` of `inode-stable`: each creates a file (every
/// other one written as a temporary and renamed to its name, as the store
/// and `--write` publish) and reads its numbers; then they are read again
/// at once and after [`INODE_WAIT`]. A file whose numbers differ at either
/// is one wrong read; the files are removed at the end.
fn inode_batch(
    base: &Path,
    first: u64,
    n: u64,
    ident: Identity<'_>,
    p: &mut Probe<'_>,
) -> Result<(), String> {
    let tree = base.join("tree");
    let mut made = Vec::new();
    let result = (|| {
        for i in first..first + n {
            let f = tree.join(format!("f{i}.h"));
            let bytes = format!("file {i}\n").into_bytes();
            if i % 2 == 0 {
                create(&f, &bytes)?;
            } else {
                let t = tree.join(format!("f{i}.h.tmp"));
                create(&t, &bytes)?;
                rename(&t, &f)?;
            }
            let id = ident(&f).map_err(|e| at(&f, &e))?;
            made.push((f, id, false));
        }
        let t0 = Instant::now();
        for wait in [Duration::ZERO, INODE_WAIT] {
            std::thread::sleep(wait.saturating_sub(t0.elapsed()));
            for (f, id, changed) in made.iter_mut().filter(|m| !m.2) {
                let now = ident(f);
                if now.as_ref().ok() == Some(id) {
                    continue;
                }
                *changed = true;
                p.wrong += 1;
                if p.first.is_none() {
                    let shown = match now {
                        Ok((d, i)) => format!("{d}:{i}"),
                        Err(e) => format!("an error: {e}"),
                    };
                    p.first = Some(format!(
                        "{} was {}:{}, then {shown} after {:.1}s",
                        f.display(),
                        id.0,
                        id.1,
                        t0.elapsed().as_secs_f64()
                    ));
                }
            }
        }
        Ok(())
    })();
    for (f, _, _) in &made {
        unlink(f)?;
    }
    result
}

/// One round of `row`. Names are reused every seven rounds, as a tree that
/// is removed and made again reuses its names; each round's contents are
/// its own.
#[allow(clippy::many_single_char_names, reason = "A, B, T, L, D as §11.2 names them")]
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
                fs::copy(&a, &b).map_err(|e| at(&b, &e))?;
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
                fs::copy(&a, &b).map_err(|e| at(&b, &e))?;
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
        "create-new-read" => create_new_read(&tree, &tmp, k, i, &pre, p)?,
        other => unreachable!("no row `{other}`"),
    }
    Ok(())
}

/// One round of `create-new-read`: the store's lock (`spec/exec.md` §2.1
/// "The lock"): a stale lock removed and the name created again in
/// place, written, synced and read by name; a claim of a name never used
/// before created, seen by a listing, removed, and no longer seen. The
/// journal is the lock's first half.
fn create_new_read(
    tree: &Path,
    tmp: &Path,
    slot: u64,
    round: u64,
    pre: &[u8],
    probe: &mut Probe<'_>,
) -> Result<(), String> {
    let lock = tree.join(format!("lock{slot}"));
    let claim = format!("{round}.claim");
    let made = tmp.join(&claim);
    unlink(&lock)?;
    create_synced(&lock, pre)?;
    probe.check(&lock, pre);
    create(&made, b"")?;
    probe.listed(tmp, &claim, true);
    fs::remove_file(&made).map_err(|e| at(&made, &e))?;
    probe.listed(tmp, &claim, false);
    probe.check(&lock, pre);
    Ok(())
}

// ---------------------------------------------------------------------------
// create-new-xproc

/// The other process of `create-new-xproc`, and its pipes.
struct Other {
    child: std::process::Child,
    to: std::process::ChildStdin,
    from: BufReader<std::process::ChildStdout>,
}

impl Other {
    fn start(reader: &[OsString], dir: &Path) -> Result<Other, String> {
        let (program, args) = reader.split_first().ok_or("no reader program")?;
        let mut child = std::process::Command::new(program)
            .args(args)
            .arg(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("the reader {}: {e}", Path::new(program).display()))?;
        let to = child.stdin.take().ok_or("the reader's stdin")?;
        let from = BufReader::new(child.stdout.take().ok_or("the reader's stdout")?);
        Ok(Other { child, to, from })
    }

    /// Sends `line` and returns the reply.
    fn ask(&mut self, line: &str) -> Result<String, String> {
        writeln!(self.to, "{line}")
            .and_then(|()| self.to.flush())
            .map_err(|e| format!("the reader: {e}"))?;
        let mut reply = String::new();
        match self.from.read_line(&mut reply) {
            Ok(0) => Err("the reader ended".to_owned()),
            Ok(_) => Ok(reply.trim_end().to_owned()),
            Err(e) => Err(format!("the reader: {e}")),
        }
    }

    fn end(self) -> Result<(), String> {
        let Other { mut child, to, from } = self;
        drop(to);
        drop(from);
        match child.wait() {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("the reader ended with {s}")),
            Err(e) => Err(format!("the reader: {e}")),
        }
    }
}

/// The bytes of round `i`'s file: `size` bytes of `i` mod 251.
fn xproc_bytes(i: u64, size: usize) -> Vec<u8> {
    vec![u8::try_from(i % 251).unwrap_or(0); size]
}

/// One round of `create-new-xproc` in `dir`.
fn xproc_round(dir: &Path, i: u64, other: &mut Other, p: &mut Probe<'_>) -> Result<(), String> {
    let size = if i.is_multiple_of(8) { XPROC_BIG } else { XPROC_SMALL };
    let claim = dir.join(format!("{i}.claim"));
    let file = dir.join(format!("j{i}"));
    create(&claim, b"")?;
    create_synced(&file, &xproc_bytes(i, size))?;
    fs::File::open(dir).and_then(|d| d.sync_all()).map_err(|e| at(dir, &e))?;
    let made = other.ask(&format!("made {i} {size}"))?;
    if made != "ok" {
        p.fail(format!("round {i}, after the creation: the other process: {made}"));
    }
    fs::remove_file(&claim).map_err(|e| at(&claim, &e))?;
    fs::remove_file(&file).map_err(|e| at(&file, &e))?;
    let gone = other.ask(&format!("gone {i}"))?;
    if gone != "ok" {
        p.fail(format!("round {i}, after the removal: the other process: {gone}"));
    }
    Ok(())
}

/// `xtask fscheck-reader [--short-at <i>] <dir>`: the other process of
/// `create-new-xproc`. Reads `made <i> <size>` (lists `<dir>`, which must
/// list `<i>.claim`; reads `<dir>/j<i>`, which must be `size` bytes of `i`
/// mod 251) and `gone <i>` (`<i>.claim` must not be listed) from stdin, and
/// answers each with `ok` or `wrong <what>`. `--short-at <i>` drops the
/// last byte of round `i`'s read: a test's planted short read.
pub fn reader_main(args: &[String]) -> ExitCode {
    let (short, dir) = match args {
        [flag, i, dir] if flag == "--short-at" => (i.parse::<u64>().ok(), dir),
        [dir] => (None, dir),
        _ => {
            eprintln!("usage: xtask fscheck-reader [--short-at <i>] <dir>");
            return ExitCode::from(2);
        }
    };
    let dir = Path::new(dir);
    let mut out = io::stdout();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { return ExitCode::FAILURE };
        let reply = reader_reply(dir, &line, short);
        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn reader_reply(dir: &Path, line: &str, short: Option<u64>) -> String {
    let words: Vec<&str> = line.split(' ').collect();
    let listed = |name: &str| {
        fs::read_dir(dir)
            .map(|e| e.filter_map(Result::ok).any(|e| e.file_name().to_str() == Some(name)))
    };
    match words[..] {
        ["made", i, size] => {
            let (Ok(i), Ok(size)) = (i.parse::<u64>(), size.parse::<usize>()) else {
                return format!("wrong: `{line}`");
            };
            match listed(&format!("{i}.claim")) {
                Ok(true) => {}
                Ok(false) => return format!("wrong: {i}.claim not listed"),
                Err(e) => return format!("wrong: {}: {e}", dir.display()),
            }
            let file = dir.join(format!("j{i}"));
            match fs::read(&file) {
                Ok(mut got) => {
                    if short == Some(i) {
                        got.pop();
                    }
                    if got.len() != size {
                        format!("wrong: {} read {} bytes, not {size}", file.display(), got.len())
                    } else if got != xproc_bytes(i, size) {
                        format!("wrong: {} read other bytes", file.display())
                    } else {
                        "ok".to_owned()
                    }
                }
                Err(e) => format!("wrong: {} read an error: {e}", file.display()),
            }
        }
        ["gone", i] => match listed(&format!("{i}.claim")) {
            Ok(false) => "ok".to_owned(),
            Ok(true) => format!("wrong: {i}.claim still listed after its removal"),
            Err(e) => format!("wrong: {}: {e}", dir.display()),
        },
        _ => format!("wrong: `{line}`"),
    }
}
