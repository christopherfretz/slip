//! The script test format and its runner (`spec/api.md` §11,
//! `docs/wp/WP-090.md`; the grammar of `expect.txt` and the decisions are
//! in `spec/landed/api/WP-090.md`).
//!
//! A script test is a directory: `transform.cpp` (the transform),
//! `t.cpp` (the subject's main file), any other file (a file of the
//! subject) and `expect.txt` (the expected rendering). [`load_script`]
//! reads a directory's header lines; [`run_group`] compiles one transform
//! once and runs it over the subjects of every directory that shares it;
//! [`run_script`] is the same for one directory; [`run_suite`] discovers,
//! groups, runs, compares and blesses.
//!
//! What a run gives is compared with `expect.txt` byte for byte, with no
//! normalisation: what differs between machines (the transform id, which
//! holds absolute paths) is left out of the rendering, never replaced
//! afterwards.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use slip_kiln::session::Cancel;
use slip_model::Digest;

use crate::driver::{self, Budgets, Failure, FailureKind, LoadError, Loaded, Outcome};
use crate::kiln_subject::KilnSubject;
use crate::output::{Place, RunOutput, Style};

/// The transform of a script test.
pub const TRANSFORM: &str = "transform.cpp";
/// The subject's main file.
pub const MAIN: &str = "t.cpp";
/// The expected rendering.
pub const EXPECT: &str = "expect.txt";

/// What a run renders on the `transform` line in place of the id: the id
/// holds the transform file's and the resource directory's absolute paths
/// (`spec/api.md` §2.2), which no committed file may spell.
pub const TRANSFORM_LINE: &str = "<id>";

/// A script test directory as its `transform.cpp` header lines describe it
/// (`spec/api.md` §11 rule 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub dir: PathBuf,
    /// `// spec:`, required: `§N` or `§N.M…`, then optionally a space and
    /// more (`§5.1 rule 2`).
    pub spec: String,
    /// `// args:`: the author's options (§2.1 rule 2), split at spaces.
    pub args: Vec<String>,
    /// `// steps:`, `// bytes:`, `// depth:`; the defaults of §2.5.
    pub budgets: Budgets,
    /// `false` with `// owned: no`: no file of the subject is editable.
    pub owned: bool,
    /// `// xfail: <reason>` (`spec/harness.md` §6).
    pub xfail: Option<String>,
    /// `// counters: a, b`: the counters the rendering keeps.
    pub counters: Vec<String>,
}

/// The header lines of a transform (`spec/api.md` §11 rule 2), without
/// the directory: also what a transform test of `spec/harness.md` §6
/// that names a `.cpp` transform reads (§11 rule 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformHeader {
    pub spec: String,
    pub args: Vec<String>,
    pub budgets: Budgets,
    pub owned: bool,
    pub xfail: Option<String>,
    pub counters: Vec<String>,
}

/// The header lines of a subject's main file (§11 rule 1, as
/// `spec/harness.md` §6.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectHeader {
    /// `// std:`; `c++20` without one.
    pub std: String,
    /// `// target:`; the snippet front end's default without one.
    pub target: Option<String>,
    /// `// args:`, split at spaces.
    pub args: Vec<String>,
}

/// The keys of a transform's header lines.
pub const TRANSFORM_KEYS: [&str; 8] =
    ["spec", "args", "steps", "bytes", "depth", "owned", "xfail", "counters"];

/// The keys of a subject's header lines.
pub const SUBJECT_KEYS: [&str; 3] = ["std", "target", "args"];

/// Whether `line` looks like a header line: `//`, any blanks, a word of
/// ASCII letters and `-` that begins with a letter, any blanks, then `:`
/// (`spec/harness.md` §6.1). A comment that reads so (`// Note: …`)
/// cannot open the file.
pub fn looks_like_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("//") else { return false };
    let rest = rest.trim_start();
    let word =
        rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == '-').len();
    word > 0
        && rest[..word].starts_with(|c: char| c.is_ascii_alphabetic())
        && rest[word..].trim_start().starts_with(':')
}

/// `line` as an exact header line of one of `keys`: `// <key>: <value>`,
/// one space after `//` and after the colon, the value not empty and not
/// beginning with a blank.
fn exact_header<'a>(line: &'a str, keys: &[&str]) -> Option<(&'a str, &'a str)> {
    let (key, value) = line.strip_prefix("// ")?.split_once(": ")?;
    (keys.contains(&key) && !value.is_empty() && !value.starts_with([' ', '\t']))
        .then_some((key, value))
}

/// The header block of `text`: its leading lines that look like header
/// lines, each `(line number, key, value)`. The block ends at the first
/// line that does not look like one. A line of the block that looks like
/// a header line and is not an exact one of `keys`, a key given twice,
/// and an exact header line after the block (after a blank line, a
/// comment, a byte-order mark, a `/* */`: it would not be read) are
/// errors.
pub fn header_block<'a>(
    text: &'a str,
    keys: &[&str],
) -> Result<Vec<(usize, &'a str, &'a str)>, String> {
    let mut out: Vec<(usize, &str, &str)> = Vec::new();
    let mut lines = text.lines().enumerate();
    for (i, line) in lines.by_ref() {
        let line = line.trim_end();
        if !looks_like_header(line) {
            if exact_header(line.trim_start_matches('\u{feff}'), keys).is_some() {
                return Err(format!("line {}: a header line outside the header block", i + 1));
            }
            break;
        }
        let Some((key, value)) = exact_header(line, keys) else {
            let keys: Vec<String> = keys.iter().map(|k| format!("`// {k}: …`")).collect();
            return Err(format!(
                "line {}: `{line}` looks like a header line but is not one of {}",
                i + 1,
                keys.join(", ")
            ));
        };
        if out.iter().any(|(_, k, _)| *k == key) {
            return Err(format!("line {}: a second `// {key}:` line", i + 1));
        }
        out.push((i + 1, key, value));
    }
    if let Some((i, _)) = lines
        .find(|(_, l)| exact_header(l.trim_end().trim_start_matches('\u{feff}'), keys).is_some())
    {
        return Err(format!("line {}: a header line outside the header block", i + 1));
    }
    Ok(out)
}

/// Whether `spec` is `§N` or `§N.M…` (decimal numbers without a leading
/// zero), then nothing or a space and more.
pub fn spec_form(spec: &str) -> bool {
    let Some(rest) = spec.strip_prefix('§') else { return false };
    let number =
        rest.split_once(' ').map_or(rest, |(n, more)| if more.is_empty() { "" } else { n });
    !number.is_empty()
        && number.split('.').all(|n| {
            !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && (n == "0" || !n.starts_with('0'))
        })
}

/// A decimal number of digits only.
fn number<T: std::str::FromStr>(line: usize, key: &str, value: &str) -> Result<T, String> {
    value
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| value.parse::<T>().ok())
        .flatten()
        .ok_or_else(|| format!("line {line}: `// {key}: {value}`: not a decimal number in range"))
}

/// Whether `name` can be a counter's name: ASCII lowercase letters,
/// digits, `_` and `.`.
fn counter_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// The header lines of a transform's text (§11 rule 2).
pub fn transform_header(text: &str) -> Result<TransformHeader, String> {
    let mut h = TransformHeader {
        spec: String::new(),
        args: Vec::new(),
        budgets: Budgets::default(),
        owned: true,
        xfail: None,
        counters: Vec::new(),
    };
    let mut spec = false;
    for (line, key, value) in header_block(text, &TRANSFORM_KEYS)? {
        match key {
            "spec" => {
                if !spec_form(value) {
                    return Err(format!(
                        "line {line}: `// spec: {value}` does not name a section as `§N.M`"
                    ));
                }
                value.clone_into(&mut h.spec);
                spec = true;
            }
            "args" => {
                h.args = value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect();
            }
            "steps" => h.budgets.steps = number(line, key, value)?,
            "bytes" => h.budgets.bytes = number(line, key, value)?,
            "depth" => h.budgets.depth = number(line, key, value)?,
            "owned" if value == "no" => h.owned = false,
            "owned" => return Err(format!("line {line}: `// owned:` takes only `no`")),
            "xfail" => h.xfail = Some(value.to_owned()),
            "counters" => {
                for name in value.split(',').map(str::trim) {
                    if !counter_name(name) {
                        return Err(format!(
                            "line {line}: `// counters: {value}`: `{name}` is not a counter's name"
                        ));
                    }
                    if h.counters.iter().any(|c| c == name) {
                        return Err(format!("line {line}: counter `{name}` is listed twice"));
                    }
                    h.counters.push(name.to_owned());
                }
            }
            _ => unreachable!("header_block admits only TRANSFORM_KEYS, not `{key}`"),
        }
    }
    if !spec {
        return Err(
            "no `// spec: §N.M` line: every script test names the section it covers".to_owned()
        );
    }
    Ok(h)
}

/// The header lines of a subject's main file (§11 rule 1).
pub fn subject_header(text: &str) -> Result<SubjectHeader, String> {
    let mut h = SubjectHeader { std: "c++20".to_owned(), target: None, args: Vec::new() };
    for (line, key, value) in header_block(text, &SUBJECT_KEYS)? {
        match key {
            "std" => value.clone_into(&mut h.std),
            "target" if !value.contains([' ', '\t']) => h.target = Some(value.to_owned()),
            "target" => return Err(format!("line {line}: `// target:` takes one word")),
            "args" => {
                h.args = value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect();
            }
            _ => unreachable!("header_block admits only SUBJECT_KEYS, not `{key}`"),
        }
    }
    Ok(h)
}

/// The script test in `dir`: the header lines of its `transform.cpp`,
/// after checking that it has a `t.cpp` whose header lines are well
/// formed. Every error names the file.
pub fn load_script(dir: &Path) -> Result<Script, String> {
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{name}: cannot be read: {e}"))
    };
    let text = read(TRANSFORM)?;
    let h = transform_header(&text).map_err(|e| format!("{TRANSFORM}: {e}"))?;
    subject_header(&read(MAIN)?).map_err(|e| format!("{MAIN}: {e}"))?;
    Ok(Script {
        dir: dir.to_path_buf(),
        spec: h.spec,
        args: h.args,
        budgets: h.budgets,
        owned: h.owned,
        xfail: h.xfail,
        counters: h.counters,
    })
}

/// What running one script test gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ran {
    /// The rendering of the result (§11 rule 3).
    Rendered(String),
    /// The test itself is wrong: a file that cannot be read, a subject
    /// with errors, a transform that reads a file of its directory.
    Error(String),
    /// The host gave no result the format can render: a `host-failure`
    /// (never the transform's, `spec/api.md` §2.8), a transform retired by
    /// an earlier run of its group, a run that was refused. A failure of
    /// the test that `xfail` does not mask.
    NoResult(String),
}

/// The stack of a group's thread: what the driver requires, and a MiB for
/// the runner's own frames above the load.
pub const GROUP_STACK: usize = driver::STACK + (1 << 20);

/// Compiles the transform of `scripts[0]` once, with its `// args:`, and
/// runs it over the subject of each of `scripts` in order, on one new
/// thread of [`GROUP_STACK`] bytes (the loaded transform runs only on the
/// thread that loaded it, §2.5 rule 5). The scripts must share their
/// `// args:` and the bytes of their `transform.cpp` ([`group_key`]):
/// one compile then stands for all (WP-090 rule 2), which nothing in a
/// rendering can tell apart.
///
/// A transform that does not load renders as the error's name for every
/// script (§11 rule 3), except `host-failure`, which is [`Ran::NoResult`].
/// Once a run retires the transform, every later script of the group is
/// [`Ran::NoResult`], naming the directory whose run retired it.
pub fn run_group(scripts: &[&Script], resource: &Path) -> Vec<Ran> {
    let Some(first) = scripts.first() else { return Vec::new() };
    let file = first.dir.join(TRANSFORM);
    let spawned = std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("script-group".to_owned())
            .stack_size(GROUP_STACK)
            .spawn_scoped(s, || {
                driver::with_transform(resource, &first.args, &file, driver::STACK, |loaded| {
                    match loaded {
                        Ok(mut l) => run_loaded(&mut l, scripts),
                        Err(e) => scripts.iter().map(|_| load_failed(&e)).collect(),
                    }
                })
            })
            .map(std::thread::ScopedJoinHandle::join)
    });
    match spawned {
        Ok(Ok(ran)) => ran,
        Ok(Err(panic)) => {
            let why = format!("the group's thread panicked: {}", panic_text(&*panic));
            scripts.iter().map(|_| Ran::NoResult(why.clone())).collect()
        }
        Err(e) => {
            let why = format!("cannot start a thread of {GROUP_STACK} bytes of stack: {e}");
            scripts.iter().map(|_| Ran::NoResult(why.clone())).collect()
        }
    }
}

/// [`run_group`] of one script: `Ok` is the rendering; `Err` says why
/// there is none (the test is wrong, or the host gave no result).
pub fn run_script(script: &Script, resource: &Path) -> Result<String, String> {
    match run_group(&[script], resource).pop() {
        Some(Ran::Rendered(text)) => Ok(text),
        Some(Ran::Error(e)) => Err(format!("the test is wrong: {e}")),
        Some(Ran::NoResult(e)) => Err(format!("no result: {e}")),
        None => unreachable!("run_group gives one result per script"),
    }
}

/// What a transform that did not load renders as (§11 rule 3):
/// `error <name>`, except `host-failure`, which is no result.
pub fn load_failed(e: &LoadError) -> Ran {
    match e {
        LoadError::Host { .. } => {
            Ran::NoResult(format!("the host failed loading the transform: {e}"))
        }
        _ => Ran::Rendered(format!("error {}\n", e.name())),
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic whose payload is not text".to_owned())
}

/// The driver's loaded transform, run script by script.
struct Driver<'a, 'l>(&'a mut Loaded<'l>);

impl GroupRun for Driver<'_, '_> {
    fn run(&mut self, script: &Script) -> Ran {
        run_one(self.0, script)
    }
    fn retired(&self) -> Option<String> {
        self.0.retired().map(str::to_owned)
    }
}

/// The loaded transform over each script's subject.
fn run_loaded(l: &mut Loaded<'_>, scripts: &[&Script]) -> Vec<Ran> {
    let first = &scripts[0].dir;
    if let Some(read) = l.files().iter().find(|f| {
        let p = Path::new(std::str::from_utf8(&f.path).unwrap_or(""));
        p.starts_with(first) && p != first.join(TRANSFORM)
    }) {
        // §1: nothing of the subject is visible to the transform; and a
        // file beside one transform.cpp would make two directories with
        // the same transform.cpp different transforms.
        let why = format!(
            "{TRANSFORM} reads `{}`, a file of its directory; the files of a script test's \
             directory are the subject's",
            String::from_utf8_lossy(&read.path)
        );
        return scripts.iter().map(|_| Ran::Error(why.clone())).collect();
    }
    run_each(&mut Driver(l), scripts)
}

/// One loaded transform, as [`run_each`] uses it: so that its rule is
/// tested without a transform that retires.
pub trait GroupRun {
    /// One script's subject, parsed, run and rendered.
    fn run(&mut self, script: &Script) -> Ran;
    /// Why the use of the transform ended, if it has.
    fn retired(&self) -> Option<String>;
}

/// Each script in order through `r`, until the transform is retired:
/// from then on each script is [`Ran::NoResult`], naming the directory
/// whose run retired it and why, and is not run.
pub fn run_each(r: &mut dyn GroupRun, scripts: &[&Script]) -> Vec<Ran> {
    let mut retired_by: Option<(String, String)> = None;
    let mut out = Vec::with_capacity(scripts.len());
    for script in scripts {
        if let Some((dir, why)) = &retired_by {
            out.push(Ran::NoResult(format!(
                "not run: the transform was retired by the run of `{dir}`: {why}"
            )));
            continue;
        }
        out.push(r.run(script));
        if let Some(why) = r.retired() {
            retired_by = Some((script.dir.display().to_string(), why));
        }
    }
    out
}

/// The files of a script test's subject: every file of its directory, at
/// any depth, but `transform.cpp` and `expect.txt` at the top, by path
/// relative to the directory (`/`-separated), in byte order.
pub fn subject_files(dir: &Path) -> Result<Vec<(String, String)>, String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, String)>) -> Result<(), String> {
        let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in rd {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let name = entry.file_name().into_string().map_err(|n| {
                format!("{}: a file name that is not UTF-8: {}", dir.display(), n.display())
            })?;
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if prefix.is_empty() && (rel == TRANSFORM || rel == EXPECT) {
                continue;
            }
            let kind = entry.file_type().map_err(|e| format!("{rel}: {e}"))?;
            if kind.is_dir() {
                walk(&entry.path(), &rel, out)?;
            } else {
                let text = std::fs::read_to_string(entry.path())
                    .map_err(|e| format!("{rel}: cannot be read as UTF-8 text: {e}"))?;
                out.push((rel, text));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out)?;
    out.sort();
    Ok(out)
}

/// One script's subject, parsed, run and rendered.
fn run_one(l: &mut Loaded<'_>, script: &Script) -> Ran {
    let files = match subject_files(&script.dir) {
        Ok(f) => f,
        Err(e) => return Ran::Error(e),
    };
    let Some(main) = files.iter().find(|(p, _)| p == MAIN) else {
        return Ran::Error(format!("{MAIN} is missing"));
    };
    let header = match subject_header(&main.1) {
        Ok(h) => h,
        Err(e) => return Ran::Error(format!("{MAIN}: {e}")),
    };
    let refs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let snippet =
        slip_kiln::snippet::Snippet { files: &refs, main: MAIN, include: &[""], std: &header.std };
    let args: Vec<&str> = header.args.iter().map(String::as_str).collect();
    let extra = slip_kiln::snippet::Extra { args: &args, target: header.target.as_deref() };
    let owned = script.owned;
    let owned = move |_: &[u8]| owned;
    let parsed = slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
        if p.errors > 0 {
            // §2.4 rule 4: such a unit is not given to a transform.
            return Ran::Error(format!(
                "the subject has {} error(s), and a unit with errors is not given to a \
                 transform (spec/api.md §2.4 rule 4):\n{}",
                p.errors, p.diagnostics
            ));
        }
        let subject = KilnSubject::from_snippet(p, &owned);
        match l.run(&subject, script.budgets, &Cancel::new()) {
            Ok((Outcome::Completed(out), _)) => {
                let names = if has_keys(&out) { key_names(p) } else { BTreeMap::new() };
                Ran::Rendered(render_completed(&out, &files, &names, &script.counters))
            }
            Ok((Outcome::Failed(f), _)) => run_failed(&f),
            Err(e) => Ran::NoResult(format!("the run gave no outcome: {e}")),
        }
    });
    parsed.unwrap_or_else(|e| Ran::Error(format!("the subject cannot be parsed: {e}")))
}

/// What a failed run renders as (§11 rule 3): [`render_failed`], except
/// `host-failure`, which is no result: the host's, never the transform's
/// (§2.8).
pub fn run_failed(f: &Failure) -> Ran {
    if f.kind == FailureKind::HostFailure {
        Ran::NoResult(format!("the run is a host-failure: {}", f.message))
    } else {
        Ran::Rendered(render_failed(f.kind, &f.message))
    }
}

/// Whether the output names a key anywhere.
fn has_keys(out: &RunOutput) -> bool {
    use crate::output::Piece;
    use slip_model::keys::Parent;
    out.edits.iter().any(|e| e.pieces.iter().any(|p| matches!(p, Piece::Spelled { .. })))
        || !out.changed.is_empty()
        || out
            .renamed
            .iter()
            .any(|r| matches!(r.parent, Parent::Unit(_)) || matches!(r.to_parent, Parent::Unit(_)))
}

/// The name of every unit of the subject (`spec/verify.md` §6.1), by key,
/// from a digest of the unit on a thread of the walk's stack.
fn key_names(p: &slip_kiln::snippet::Parsed<'_>) -> BTreeMap<Digest, String> {
    let paths: Vec<Vec<u8>> =
        p.sm.file_ids()
            .map(|f| p.path(f).map(|s| s.as_bytes().to_vec()).unwrap_or_default())
            .collect();
    let (ast, sm) = (p.ast, p.sm);
    let walked = std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("script-names".to_owned())
            .stack_size(slip_digest::STACK_SIZE)
            .spawn_scoped(s, || {
                let path = |f: slip_kiln::kiln::source::FileId| {
                    paths.get(f.index()).cloned().unwrap_or_default()
                };
                let rules = slip_model::keys::RuleTable::empty();
                let input = slip_digest::Input { ast, sm, subject_path: &path };
                let options = slip_digest::Options {
                    rules: &rules,
                    // The empty table reads no ownership (verify.md §2.1).
                    read_only: slip_digest::ReadOnly::EVERY_FILE,
                    detail: slip_digest::Detail::None,
                    names: true,
                    rule_use: false,
                };
                slip_digest::digest(&input, &options).names
            })
            .map(std::thread::ScopedJoinHandle::join)
    });
    match walked {
        Ok(Ok(names)) => names.into_iter().collect(),
        // No name: the keys are written as digests, which a rendering
        // still compares exactly.
        _ => BTreeMap::new(),
    }
}

/// The rendering of a completed run (§11 rule 3): [`RunOutput::render`]
/// with the transform line [`TRANSFORM_LINE`], places as
/// `file:line:col-line:col`, keys by their names, and only the counters
/// listed (`// counters:`), in the order of the output; a listed counter
/// the output does not have is `counter <name> absent`.
pub fn render_completed(
    out: &RunOutput,
    files: &[(String, String)],
    names: &BTreeMap<Digest, String>,
    counters: &[String],
) -> String {
    let mut kept = out.clone();
    kept.counters.retain(|c| counters.contains(&c.name));
    let by_path: BTreeMap<&[u8], &[u8]> =
        files.iter().map(|(p, t)| (p.as_bytes(), t.as_bytes())).collect();
    let position = |place: &Place| match by_path.get(place.file.path.as_slice()) {
        Some(bytes) => {
            let (l1, c1) = line_col(bytes, place.lo);
            let (l2, c2) = line_col(bytes, place.hi);
            format!(":{l1}:{c1}-{l2}:{c2}")
        }
        None => format!(":{}-{}", place.lo, place.hi),
    };
    let key_name = |k: &Digest| names.get(k).map(|n| n.as_bytes().to_vec());
    let style = Style {
        transform: Some(TRANSFORM_LINE),
        position: Some(&position),
        key_name: Some(&key_name),
    };
    let mut text = String::new();
    let _ = kept.render_styled(&mut text, &style);
    for name in counters {
        if !out.counters.iter().any(|c| &c.name == name) {
            let _ = writeln!(text, "counter {name} absent");
        }
    }
    text
}

/// The line and column (both from 1, the column in bytes) of byte offset
/// `at` of `bytes`: of the byte at `at`, or of the end when `at` is the
/// length. An offset past the end counts as the end.
pub fn line_col(bytes: &[u8], at: u32) -> (usize, usize) {
    let at = (at as usize).min(bytes.len());
    let before = &bytes[..at];
    #[allow(clippy::naive_bytecount, reason = "one place per line of expect.txt; no crate for it")]
    let line = 1 + before.iter().filter(|&&b| b == b'\n').count();
    let col = 1 + before.iter().rev().take_while(|&&b| b != b'\n').count();
    (line, col)
}

/// The rendering of a failed run (§11 rule 3): `failed <kind> "<first
/// line of the message>"`, quoted as [`RunOutput::render`] quotes text.
pub fn render_failed(kind: FailureKind, message: &str) -> String {
    let first = message.lines().next().unwrap_or("");
    let mut text = format!("failed {} \"", kind.name());
    for &b in first.as_bytes() {
        match b {
            b'\\' => text.push_str("\\\\"),
            b'"' => text.push_str("\\\""),
            b'\t' => text.push_str("\\t"),
            0x20..=0x7e => text.push(b as char),
            _ => {
                let _ = write!(text, "\\x{b:02x}");
            }
        }
    }
    text.push_str("\"\n");
    text
}

// ---------------------------------------------------------------------------
// The suite.

/// Every script test directory under `root`: the directories that hold a
/// `transform.cpp`, in byte order of their paths. Every file under `root`
/// must lie in one; none lies inside another; there is at least one.
pub fn discover(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, inside: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        let test = dir.join(TRANSFORM).is_file();
        if test && inside {
            return Err(format!("{}: a script test inside another one's directory", dir.display()));
        }
        if test {
            out.push(dir.to_path_buf());
        }
        for e in entries {
            let kind = e.file_type().map_err(|err| format!("{}: {err}", e.path().display()))?;
            if kind.is_dir() {
                walk(&e.path(), inside || test, out)?;
            } else if !(inside || test) {
                return Err(format!(
                    "{}: a file outside every script test directory (one with a {TRANSFORM})",
                    e.path().display()
                ));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, false, &mut out)?;
    if out.is_empty() {
        return Err(format!(
            "{}: no script test directory (one with a {TRANSFORM})",
            root.display()
        ));
    }
    out.sort();
    Ok(out)
}

/// The key of a group: the scripts that share it share one compile
/// (`// args:` and the bytes of `transform.cpp`, WP-090 rule 2).
pub fn group_key(script: &Script) -> Result<(Vec<String>, Vec<u8>), String> {
    let bytes = std::fs::read(script.dir.join(TRANSFORM))
        .map_err(|e| format!("{TRANSFORM}: cannot be read: {e}"))?;
    Ok((script.args.clone(), bytes))
}

/// The groups of `keyed` (each an index, its [`group_key`] and its
/// script): those with one key, in the order given, the groups in the
/// order of their first index.
pub fn groups<K: Ord>(keyed: Vec<(usize, K, Script)>) -> Vec<Vec<(usize, Script)>> {
    let mut by_key: BTreeMap<K, Vec<(usize, Script)>> = BTreeMap::new();
    for (i, key, script) in keyed {
        by_key.entry(key).or_default().push((i, script));
    }
    let mut out: Vec<Vec<(usize, Script)>> = by_key.into_values().collect();
    out.sort_by_key(|g| g[0].0);
    out
}

/// The outcome of one script test (`spec/harness.md` §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    /// The rendering differs from `expect.txt`, or the host gave no
    /// result: what was wrong.
    Fail(String),
    /// It differs and the test says `// xfail:`.
    Xfail(String),
    /// It is as expected and the test says `// xfail:`: fails the suite.
    Xpass,
    /// The test itself is wrong: fails the suite.
    Error(String),
    /// `SLIP_BLESS=1` wrote `expect.txt`.
    Blessed,
}

impl Verdict {
    /// `PASS`, `FAIL`, `XFAIL`, `XPASS`, `ERROR`, `BLESSED`.
    pub fn name(&self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail(_) => "FAIL",
            Verdict::Xfail(_) => "XFAIL",
            Verdict::Xpass => "XPASS",
            Verdict::Error(_) => "ERROR",
            Verdict::Blessed => "BLESSED",
        }
    }

    /// Whether the suite fails with this outcome.
    pub fn fails(&self) -> bool {
        matches!(self, Verdict::Fail(_) | Verdict::Xpass | Verdict::Error(_))
    }
}

/// What to do with the rendering of one test.
///
/// `expect` is `expect.txt`'s bytes, `None` when it is missing. Without
/// `bless`, a missing file is an error of the test, and the rendering is
/// compared with the file byte for byte. With `bless`, a test that is not
/// `xfail` gets the rendering written (the second value, when it differs
/// from what is there); an `xfail` test is judged as without, since its
/// `expect.txt` holds what the host does not give yet.
pub fn judge(
    xfail: Option<&str>,
    expect: Option<&[u8]>,
    ran: Ran,
    bless: bool,
) -> (Verdict, Option<String>) {
    let text = match ran {
        Ran::Rendered(text) => text,
        Ran::Error(e) => return (Verdict::Error(e), None),
        Ran::NoResult(e) => return (Verdict::Fail(e), None),
    };
    if bless && xfail.is_none() {
        let write = (expect != Some(text.as_bytes())).then_some(text);
        return (if write.is_some() { Verdict::Blessed } else { Verdict::Pass }, write);
    }
    let Some(expect) = expect else {
        return (
            Verdict::Error(format!(
                "{EXPECT} is missing; SLIP_BLESS=1 writes it, and a blessed file is reviewed like \
                 code. The rendering:\n{text}"
            )),
            None,
        );
    };
    let same = expect == text.as_bytes();
    let verdict = match (same, xfail) {
        (true, None) => Verdict::Pass,
        (true, Some(_)) => Verdict::Xpass,
        (false, None) => Verdict::Fail(difference(expect, &text)),
        (false, Some(reason)) => Verdict::Xfail(reason.to_owned()),
    };
    (verdict, None)
}

/// The first difference between `expect.txt` and the rendering, then the
/// whole rendering.
pub fn difference(expect: &[u8], text: &str) -> String {
    let expect = String::from_utf8_lossy(expect);
    let (mut expected, mut actual) = (expect.split_inclusive('\n'), text.split_inclusive('\n'));
    let mut line = 1;
    loop {
        match (expected.next(), actual.next()) {
            (Some(want), Some(got)) if want == got => line += 1,
            (want, got) => {
                let show = |l: Option<&str>| l.map_or("(nothing)".to_owned(), |l| format!("{l:?}"));
                return format!(
                    "{EXPECT} differs at line {line}:\n  expected: {}\n  actual:   {}\nthe rendering:\n{text}",
                    show(want),
                    show(got)
                );
            }
        }
    }
}

/// One script test's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// Its directory relative to the suite's root, `/`-separated.
    pub name: String,
    pub verdict: Verdict,
}

/// How [`run_suite`] runs.
#[derive(Debug, Clone, Default)]
pub struct SuiteOptions {
    /// Run only the tests whose name contains one of these (or, with
    /// `exact`, is one of these); none: every test.
    pub filters: Vec<String>,
    pub exact: bool,
    /// Groups run at once (at least one).
    pub threads: usize,
    /// `SLIP_BLESS=1`.
    pub bless: bool,
}

/// `SLIP_BLESS` from its value: unset is `false`, `1` is `true`, anything
/// else is an error (a test run never guesses whether it should write).
pub fn bless_from(var: Option<&std::ffi::OsStr>) -> Result<bool, String> {
    match var {
        None => Ok(false),
        Some(v) if v == "1" => Ok(true),
        Some(v) => Err(format!("SLIP_BLESS={}: the only value is 1", v.to_string_lossy())),
    }
}

/// Whether the filters select the test `name`: no filter selects every
/// test; else one filter must be in the name or, with `exact`, be it
/// (libtest's rule).
pub fn selects(filters: &[String], exact: bool, name: &str) -> bool {
    filters.is_empty()
        || filters.iter().any(|f| if exact { name == f } else { name.contains(f.as_str()) })
}

/// The name of a test directory under `root`.
pub fn case_name(root: &Path, dir: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Runs the script tests under `root` with the resource directory
/// `resource`: discovers them ([`discover`]: an error ends the suite),
/// keeps those `opts` selects, loads each ([`load_script`]: a directory
/// that cannot be loaded is `ERROR`), groups them by [`group_key`], runs
/// the groups on `opts.threads` threads, each group holding what
/// `token` gives while it runs (a slot of the jobserver), and judges each
/// test ([`judge`]), writing `expect.txt` when blessing. `report` is
/// called with each case as it is decided; the cases come back in
/// directory order.
pub fn run_suite(
    root: &Path,
    resource: &Path,
    opts: &SuiteOptions,
    token: &(dyn Fn() -> Box<dyn std::any::Any> + Sync),
    report: &(dyn Fn(&Case) + Sync),
) -> Result<Vec<Case>, String> {
    let dirs: Vec<PathBuf> = discover(root)?
        .into_iter()
        .filter(|d| selects(&opts.filters, opts.exact, &case_name(root, d)))
        .collect();
    let mut cases: Vec<Option<Case>> = dirs.iter().map(|_| None).collect();
    let mut keyed = Vec::new();
    for (i, dir) in dirs.iter().enumerate() {
        match load_script(dir).and_then(|s| group_key(&s).map(|k| (k, s))) {
            Ok((key, script)) => keyed.push((i, key, script)),
            Err(e) => {
                let case = Case { name: case_name(root, dir), verdict: Verdict::Error(e) };
                report(&case);
                cases[i] = Some(case);
            }
        }
    }
    let groups = groups(keyed);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let decided = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..opts.threads.max(1).min(groups.len()) {
            s.spawn(|| {
                loop {
                    let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(group) = groups.get(n) else { break };
                    let held = token();
                    let scripts: Vec<&Script> = group.iter().map(|(_, s)| s).collect();
                    let ran = run_group(&scripts, resource);
                    drop(held);
                    for ((i, script), ran) in group.iter().zip(ran) {
                        let case = decide(root, script, ran, opts.bless);
                        report(&case);
                        decided
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push((*i, case));
                    }
                }
            });
        }
    });
    for (i, case) in decided.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner) {
        cases[i] = Some(case);
    }
    Ok(cases
        .into_iter()
        .map(|c| c.unwrap_or_else(|| unreachable!("every selected directory is decided")))
        .collect())
}

/// Judges one test and blesses it when asked.
fn decide(root: &Path, script: &Script, ran: Ran, bless: bool) -> Case {
    let name = case_name(root, &script.dir);
    let path = script.dir.join(EXPECT);
    let expect = match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Case {
                name,
                verdict: Verdict::Error(format!("{EXPECT}: cannot be read: {e}")),
            };
        }
    };
    let (verdict, write) = judge(script.xfail.as_deref(), expect.as_deref(), ran, bless);
    if let Some(text) = write
        && let Err(e) = std::fs::write(&path, text)
    {
        return Case { name, verdict: Verdict::Error(format!("{EXPECT}: cannot be written: {e}")) };
    }
    Case { name, verdict }
}

/// The count line of a suite: `api: <n> passed, <n> failed, …` with every
/// outcome that occurred, in the order of [`Verdict::name`]'s list, and
/// whether the suite passed.
pub fn summary(cases: &[Case]) -> (String, bool) {
    let mut parts = Vec::new();
    for (name, word) in [
        ("PASS", "passed"),
        ("FAIL", "failed"),
        ("XFAIL", "xfailed"),
        ("XPASS", "xpassed"),
        ("ERROR", "errors"),
        ("BLESSED", "blessed"),
    ] {
        let n = cases.iter().filter(|c| c.verdict.name() == name).count();
        if n > 0 || name == "PASS" {
            parts.push(format!("{n} {word}"));
        }
    }
    let ok = !cases.iter().any(|c| c.verdict.fails());
    (format!("api: {}", parts.join(", ")), ok)
}
