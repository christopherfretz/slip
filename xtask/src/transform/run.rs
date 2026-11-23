//! One transform test, run and judged (`spec/harness.md` §6.3): the work
//! directory, the database, every command line given to `slip` (built in
//! [`cmdline`] and nowhere else), reading a run's output, the comparison
//! with the golden files, `--bless`, and the order of the oracles.

use super::oracles;
use super::test_file::{Allow, Expected, Oracle, Test};
use super::tree::{self, Files};
use super::{Outcome, Report};
use slip_model::json::{self, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The command lines the runner gives `slip`, in one place, so that the
/// command packages can be checked against them
/// (`spec/landed/harness/WP-077.md` §2).
pub mod cmdline {
    use super::{Allow, Ctx};
    use std::ffi::OsString;
    use std::path::Path;

    /// `slip <run> --db <work>/cdb.json --owned <tree>/<p>… --readonly
    /// <tree>/<p>… --store <store> --relative-to <tree> --write -j <jobs>
    /// --allow-<v>…` (§6.3 step 2).
    pub fn transform(ctx: &Ctx<'_>, run: &[String], store: &Path, jobs: u32) -> Vec<OsString> {
        let mut a: Vec<OsString> = run.iter().map(OsString::from).collect();
        a.push("--db".into());
        a.push(ctx.db.clone().into());
        for p in &ctx.test.owned {
            a.push("--owned".into());
            a.push(ctx.tree_path(p).into());
        }
        for p in &ctx.test.readonly {
            a.push("--readonly".into());
            a.push(ctx.tree_path(p).into());
        }
        a.push("--store".into());
        a.push(store.into());
        a.push("--relative-to".into());
        a.push(ctx.tree.clone().into());
        a.push("--write".into());
        a.push("-j".into());
        a.push(jobs.to_string().into());
        for allow in &ctx.test.allow {
            a.push(format!("--allow-{}", allow.name()).into());
        }
        a
    }

    /// `slip check --plan <plan id> --round <n> --edits <edit set>
    /// [--declared <declared id>] --store <store> -j 2 [--allow-unparsed]`
    /// (§6.4 `verify` and `mutation`; `spec/edits.md` §10.2,
    /// `spec/exec.md` §8).
    pub fn check(
        ctx: &Ctx<'_>,
        store: &Path,
        plan: &str,
        round: &str,
        edits: &OsString,
        declared: Option<&str>,
    ) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec![
            "check".into(),
            "--plan".into(),
            plan.into(),
            "--round".into(),
            round.into(),
            "--edits".into(),
            edits.clone(),
        ];
        if let Some(d) = declared {
            a.push("--declared".into());
            a.push(d.into());
        }
        a.push("--store".into());
        a.push(store.into());
        a.push("-j".into());
        a.push("2".into());
        if ctx.test.allows(Allow::Unparsed) {
            a.push("--allow-unparsed".into());
        }
        a
    }

    /// The inverse of a rename (§6.4 `back`): `rename --from a::f --to g`
    /// becomes `rename --from a::g --to f`, every other word kept. `None`
    /// for a run that is not a rename with one `--from <name>` and one
    /// `--to <identifier>`.
    pub fn inverse(run: &[String]) -> Option<Vec<String>> {
        if run.first().map(String::as_str) != Some("rename") {
            return None;
        }
        let at = |opt: &str| -> Option<usize> {
            let mut found = run.iter().enumerate().filter(|(_, w)| *w == opt).map(|(i, _)| i);
            let i = found.next()?;
            if found.next().is_some() || run.iter().any(|w| w.starts_with(&format!("{opt}="))) {
                return None;
            }
            (i + 1 < run.len()).then_some(i + 1)
        };
        let (from, to) = (at("--from")?, at("--to")?);
        let old = &run[from];
        let new = &run[to];
        if new.contains("::") || new.is_empty() {
            return None;
        }
        let (inverse_from, inverse_to) = match old.rsplit_once("::") {
            Some((prefix, last)) if !last.is_empty() => {
                (format!("{prefix}::{new}"), last.to_owned())
            }
            Some(_) => return None,
            None => (new.clone(), old.clone()),
        };
        let mut out = run.to_vec();
        out[from] = inverse_from;
        out[to] = inverse_to;
        Some(out)
    }
}

/// A test being run: its paths.
#[derive(Debug)]
pub struct Ctx<'a> {
    pub test: &'a Test,
    pub slip: &'a Path,
    /// The work directory, canonical.
    pub work: PathBuf,
    /// `<work>/tree`: the copy of `pre/` every `slip` runs in.
    pub tree: PathBuf,
    /// `<work>/cdb.json`.
    pub db: PathBuf,
    /// `<work>/store`.
    pub store: PathBuf,
    /// The commands of the database: (directory, argv).
    pub commands: Vec<(PathBuf, Vec<String>)>,
    pub pre: Files,
}

impl Ctx<'_> {
    /// A path of `owned`/`readonly` in the tree; `.` is the tree.
    pub fn tree_path(&self, p: &str) -> PathBuf {
        if p == "." { self.tree.clone() } else { self.tree.join(p) }
    }

    /// Removes the tree and copies `pre/` to it again.
    pub fn restore_tree(&self) -> Result<(), String> {
        tree::remove(&self.tree).map_err(|e| format!("{}: {e}", self.tree.display()))?;
        tree::copy_tree(&self.test.dir.join("pre"), &self.tree)
            .map_err(|e| format!("copying pre/ to {}: {e}", self.tree.display()))
    }

    pub fn read_tree(&self) -> Result<Files, String> {
        tree::read_tree(&self.tree).map_err(|e| format!("the tree: {e}"))
    }

    /// Runs `slip` with `args` in the tree.
    pub fn slip(&self, args: &[OsString]) -> Result<Ran, Outcome> {
        let out = Command::new(self.slip)
            .args(args)
            .current_dir(&self.tree)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Outcome::Error(format!("cannot run `{}`: {e}", self.slip.display())))?;
        let shown = args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
        if out.status.code().is_none() {
            use std::os::unix::process::ExitStatusExt as _;
            let signal = out.status.signal().unwrap_or(0);
            return Err(Outcome::Killed(format!("`slip {shown}` killed by signal {signal}")));
        }
        Ok(Ran {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            lines: Lines::read(&String::from_utf8_lossy(&out.stdout)),
        })
    }
}

/// One finished invocation of `slip`.
#[derive(Debug, Clone)]
pub struct Ran {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub lines: Lines,
}

/// The ids and the state a run printed (`spec/edits.md` §10.5, the plan
/// line of `spec/exec.md` §8.1). Each is the first such line, except
/// `state`, which is the last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines {
    pub plan: Option<String>,
    pub transform: Option<String>,
    pub round: Option<String>,
    pub edits: Option<String>,
    pub declared: Option<String>,
    pub overlay: Option<String>,
    pub result: Option<String>,
    pub state: Option<String>,
}

impl Lines {
    pub fn read(stdout: &str) -> Lines {
        let mut l = Lines::default();
        for line in stdout.lines() {
            let Some((word, rest)) = line.split_once(' ') else { continue };
            // `declared <id>: …` and `overlay <id>: …` end their id at `:`.
            let value = rest.split_once(':').map_or(rest, |(v, _)| v);
            let slot = match word {
                "plan" => &mut l.plan,
                "transform" => &mut l.transform,
                "round" => &mut l.round,
                "edits" => &mut l.edits,
                "declared" => &mut l.declared,
                "overlay" => &mut l.overlay,
                "result" => &mut l.result,
                "state" => {
                    l.state = Some(rest.to_owned());
                    continue;
                }
                _ => continue,
            };
            let is_word = !value.is_empty() && !value.contains(' ');
            if slot.is_none() && is_word {
                *slot = Some(value.to_owned());
            }
        }
        l
    }

    /// The ids the `determinism` oracle compares, by name.
    pub fn ids(&self) -> [(&'static str, &Option<String>); 5] {
        [
            ("transform", &self.transform),
            ("edit set", &self.edits),
            ("declared", &self.declared),
            ("overlay", &self.overlay),
            ("result", &self.result),
        ]
    }
}

/// §6.3 step 3: every 64-digit id (a maximal run of exactly 64 lower-case
/// hexadecimal digits) becomes `<id>`, the store's path `@STORE@` and the
/// tree's path `@ROOT@`.
pub fn normalise(stdout: &str, tree: &Path, store: &Path) -> String {
    let mut s = stdout.to_owned();
    let (tree, store) = (tree.to_string_lossy(), store.to_string_lossy());
    // The longer first, so that neither is replaced inside the other.
    let mut paths = [(store.as_ref(), "@STORE@"), (tree.as_ref(), "@ROOT@")];
    paths.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    for (p, with) in paths {
        if !p.is_empty() {
            s = s.replace(p, with);
        }
    }
    replace_ids(&s)
}

fn replace_ids(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if hex(b[i]) {
            let start = i;
            while i < b.len() && hex(b[i]) {
                i += 1;
            }
            if i - start == 64 {
                out.push_str("<id>");
            } else {
                out.push_str(&s[start..i]);
            }
        } else {
            let start = i;
            while i < b.len() && !hex(b[i]) {
                i += 1;
            }
            out.push_str(&s[start..i]);
        }
    }
    out
}

/// The first difference of `got` from `expected`, line by line, as
/// ``output line <n>: expected `…`, got `…` `` (`the end` past the last).
pub fn first_line_difference(expected: &str, got: &str) -> Option<String> {
    if expected == got {
        return None;
    }
    let (e, g): (Vec<&str>, Vec<&str>) =
        (expected.split('\n').collect(), got.split('\n').collect());
    for i in 0..e.len().max(g.len()) {
        let shown = |l: Option<&&str>| l.map_or("the end".to_owned(), |l| format!("`{l}`"));
        if e.get(i) != g.get(i) {
            return Some(format!(
                "output line {}: expected {}, got {}",
                i + 1,
                shown(e.get(i)),
                shown(g.get(i))
            ));
        }
    }
    unreachable!("two different strings differ in a line")
}

/// Runs and judges `test` (§6.3, §6.4) in `work`; see [`super::run_one`].
pub fn run_test(
    test: &Test,
    slip: &Path,
    work: &Path,
    clang: Option<&Path>,
    bless: bool,
) -> Report {
    let mut report = Report { outcome: Outcome::Pass, clang_skipped: false };
    let judged = judge(test, slip, work, clang, bless, &mut report.clang_skipped);
    report.outcome = match (judged, &test.xfail) {
        (Ok(()), None) => Outcome::Pass,
        (Ok(()), Some(_)) => Outcome::XPass,
        (Err(Outcome::Fail(_)), Some(_)) => Outcome::XFail,
        (Err(o), _) => o,
    };
    report
}

fn error<E: std::fmt::Display>(e: E) -> Outcome {
    Outcome::Error(e.to_string())
}

/// `Ok` for a pass; `Err` with the outcome otherwise (before `xfail`).
fn judge(
    test: &Test,
    slip: &Path,
    work: &Path,
    clang: Option<&Path>,
    bless: bool,
    clang_skipped: &mut bool,
) -> Result<(), Outcome> {
    tree::check_work_dir(work, &test.area, &test.name, &test.dir).map_err(Outcome::Error)?;
    if !test.expect && !bless {
        return Err(Outcome::Error("no expect.txt (`--bless` writes it)".into()));
    }
    let pre_dir = test.dir.join("pre");
    let pre = tree::read_tree(&pre_dir).map_err(|e| Outcome::Error(format!("pre/: {e}")))?;
    let post = if test.dir.join("post").is_dir() {
        tree::read_tree(&test.dir.join("post"))
            .map_err(|e| Outcome::Error(format!("post/: {e}")))?
    } else {
        Files::new()
    };
    let work = tree::fresh_dir(work).map_err(|e| error(format!("{}: {e}", work.display())))?;
    let tree_dir = work.join("tree");
    let mut ctx = Ctx {
        test,
        slip,
        tree: tree_dir.clone(),
        db: work.join("cdb.json"),
        store: work.join("store"),
        work,
        commands: Vec::new(),
        pre: pre.clone(),
    };
    tree::copy_tree(&pre_dir, &ctx.tree).map_err(|e| error(format!("copying pre/: {e}")))?;
    ctx.commands = write_database(&ctx)?;

    // The clang oracle's pre-image, while the tree is still pre/ (§6.4).
    let clang_on = test.oracle_on(Oracle::Clang);
    if clang_on
        && test.state.may_write()
        && let Some(clang) = clang
        && let Err(e) = oracles::clang(&ctx, clang)
    {
        return Err(Outcome::Error(format!("the pre-image is not accepted by clang: {e}")));
    }

    // §6.3 step 2: the run.
    let args = cmdline::transform(&ctx, &test.run, &ctx.store, 2);
    let ran = ctx.slip(&args)?;
    let got = ctx.read_tree().map_err(Outcome::Error)?;

    // Step 3: the goldens.
    if bless {
        bless_goldens(&ctx, &ran, &got, &post)?;
    } else {
        let expect = std::fs::read(test.dir.join("expect.txt"))
            .map_err(|e| error(format!("expect.txt: {e}")))?;
        let expect = String::from_utf8_lossy(&expect);
        let differences = compare(&ctx, &ran, &expect, &tree::overlay(&pre, &post), &got);
        if !differences.is_empty() {
            return Err(Outcome::Fail(differences.join("; ")));
        }
    }

    // Step 4: the oracles.
    let failures = oracles::run_all(&ctx, &ran, got != pre, clang, clang_skipped)?;
    if failures.is_empty() { Ok(()) } else { Err(Outcome::Fail(failures.join("; "))) }
}

/// Step 3's differences: the exit status, the state, the first line of
/// the output, the first file of the tree.
fn compare(
    ctx: &Ctx<'_>,
    ran: &Ran,
    expect: &str,
    expected_tree: &Files,
    got: &Files,
) -> Vec<String> {
    let test = ctx.test;
    let mut d = Vec::new();
    if ran.status != test.exit {
        d.push(format!("exit status {}, expected {}", ran.status, test.exit));
    }
    if let Some(s) = state_difference(test.state, ran) {
        d.push(s);
    }
    let out = normalise(&ran.stdout, &ctx.tree, &ctx.store);
    if let Some(s) = first_line_difference(expect, &out) {
        d.push(s);
    }
    if let Some(s) = tree::first_difference(expected_tree, got) {
        if !test.state.may_write() && *got != ctx.pre {
            d.push(format!(
                "the tree was written, and the state `{}` does not permit writing: {s}",
                test.state.name()
            ));
        } else {
            d.push(format!("tree: {s}"));
        }
    }
    d
}

/// The state's difference, if any: a state needs a `result` line (else the
/// standard error is shown) and the last `state` line.
pub fn state_difference(expected: Expected, ran: &Ran) -> Option<String> {
    if !expected.is_state() {
        return None;
    }
    if ran.lines.result.is_none() {
        return Some(format!(
            "the run printed no result line; standard error:\n{}",
            ran.stderr.trim_end()
        ));
    }
    let got = ran.lines.state.as_deref().unwrap_or("none");
    (got != expected.name()).then(|| format!("state {got}, expected {}", expected.name()))
}

/// `--bless` (§6.3): only when the state and the exit status are as
/// `test.txt` says, and when the written tree is one `post/` can describe.
fn bless_goldens(ctx: &Ctx<'_>, ran: &Ran, got: &Files, old_post: &Files) -> Result<(), Outcome> {
    let test = ctx.test;
    let mut refuse = Vec::new();
    if ran.status != test.exit {
        refuse.push(format!("exit status {}, expected {}", ran.status, test.exit));
    }
    if let Some(s) = state_difference(test.state, ran) {
        refuse.push(s);
    }
    let added_or_missing = ctx.pre.keys().any(|k| !got.contains_key(k))
        || got.keys().any(|k| !ctx.pre.contains_key(k));
    if added_or_missing {
        let d = tree::first_difference(&ctx.pre, got).unwrap_or_default();
        refuse.push(format!("post/ cannot hold the written tree: {d}"));
    }
    if !test.state.may_write() && *got != ctx.pre {
        refuse.push(format!("the state `{}` does not permit writing", test.state.name()));
    }
    if !refuse.is_empty() {
        return Err(Outcome::Fail(format!("cannot bless: {}", refuse.join("; "))));
    }
    let io = |p: &Path, e: std::io::Error| Outcome::Error(format!("bless: {}: {e}", p.display()));
    let expect_path = test.dir.join("expect.txt");
    let out = normalise(&ran.stdout, &ctx.tree, &ctx.store);
    if std::fs::read(&expect_path).ok().as_deref() != Some(out.as_bytes()) {
        std::fs::write(&expect_path, &out).map_err(|e| io(&expect_path, e))?;
        println!("bless: {}", expect_path.display());
    }
    let post_dir = test.dir.join("post");
    let new_post: Files = got
        .iter()
        .filter(|(k, v)| ctx.pre.get(*k) != Some(v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (k, v) in &new_post {
        if old_post.get(k) != Some(v) {
            let p = post_dir.join(k);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
            }
            std::fs::write(&p, v).map_err(|e| io(&p, e))?;
            println!("bless: {}", p.display());
        }
    }
    for k in old_post.keys().filter(|k| !new_post.contains_key(*k)) {
        let p = post_dir.join(k);
        std::fs::remove_file(&p).map_err(|e| io(&p, e))?;
        println!("bless: {} (removed)", p.display());
        // Directories left empty go too, up to post/ itself.
        let mut dir = p.parent();
        while let Some(d) = dir {
            if !d.starts_with(&post_dir) || std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    Ok(())
}

/// Writes `<work>/cdb.json`: the test's `cdb.json` with `@ROOT@` replaced
/// by the tree's path, or the default commands of §6.2. Returns the
/// commands as (directory, argv), for the clang oracle.
fn write_database(ctx: &Ctx<'_>) -> Result<Vec<(PathBuf, Vec<String>)>, Outcome> {
    let tree = ctx.tree.to_str().ok_or_else(|| error("the tree's path is not UTF-8"))?;
    if tree.bytes().any(|b| b == b'"' || b == b'\\' || b < 0x20) {
        return Err(error(format!("the tree's path `{tree}` cannot be written in JSON")));
    }
    let text = if ctx.test.cdb {
        let raw = std::fs::read(ctx.test.dir.join("cdb.json"))
            .map_err(|e| error(format!("cdb.json: {e}")))?;
        let raw = String::from_utf8(raw).map_err(|_| error("cdb.json: not UTF-8"))?;
        raw.replace("@ROOT@", tree)
    } else {
        let entries: Vec<Value> = ctx
            .test
            .tu
            .iter()
            .map(|file| {
                let mut argv = vec!["kiln".to_owned(), format!("-std={}", ctx.test.std)];
                argv.extend(ctx.test.args.iter().cloned());
                argv.extend([
                    format!("-I{tree}"),
                    format!("-I{tree}/dep"),
                    "-c".into(),
                    file.clone(),
                ]);
                Value::Object(vec![
                    ("directory".into(), Value::Str(tree.to_owned())),
                    ("arguments".into(), Value::Array(argv.into_iter().map(Value::Str).collect())),
                    ("file".into(), Value::Str(file.clone())),
                ])
            })
            .collect();
        json::write_pretty(&Value::Array(entries))
            .map_err(|e| error(format!("writing the database: {e}")))?
    };
    let commands = read_commands(&text).map_err(|e| error(format!("cdb.json: {e}")))?;
    std::fs::write(&ctx.db, &text).map_err(|e| error(format!("{}: {e}", ctx.db.display())))?;
    Ok(commands)
}

/// The commands of a compilation database: each entry's `directory` and
/// its `arguments`, or its `command` split at white space with single and
/// double quotes and backslash escapes (a POSIX shell's words, without
/// expansions).
pub fn read_commands(text: &str) -> Result<Vec<(PathBuf, Vec<String>)>, String> {
    let v = json::parse(text.as_bytes()).map_err(|e| e.to_string())?;
    let entries = v.as_array().ok_or("not an array")?;
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let dir =
            e.get("directory").and_then(Value::as_str).ok_or(format!("entry {i}: no directory"))?;
        let argv = if let Some(a) = e.get("arguments").and_then(Value::as_array) {
            a.iter()
                .map(|w| {
                    w.as_str()
                        .map(str::to_owned)
                        .ok_or(format!("entry {i}: an argument is not a string"))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(c) = e.get("command").and_then(Value::as_str) {
            split_command(c).ok_or(format!("entry {i}: an unterminated quote"))?
        } else {
            return Err(format!("entry {i}: no arguments and no command"));
        };
        if argv.is_empty() {
            return Err(format!("entry {i}: an empty command"));
        }
        out.push((PathBuf::from(dir), argv));
    }
    Ok(out)
}

fn split_command(c: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = c.chars();
    while let Some(ch) = chars.next() {
        match ch {
            ' ' | '\t' | '\n' => {
                if in_word {
                    out.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\\' => {
                word.push(chars.next()?);
                in_word = true;
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        x => word.push(x),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => word.push(chars.next()?),
                        x => word.push(x),
                    }
                }
            }
            x => {
                word.push(x);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(word);
    }
    Some(out)
}

/// The store of an oracle's own run: `<work>/<sub>/store`, `<sub>` made.
pub fn oracle_store(ctx: &Ctx<'_>, sub: &str) -> Result<PathBuf, Outcome> {
    let dir = ctx.work.join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| error(format!("{}: {e}", dir.display())))?;
    Ok(dir.join("store"))
}
