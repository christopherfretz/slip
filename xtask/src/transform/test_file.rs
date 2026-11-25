//! A transform test's directory and its `test.txt` (`spec/harness.md`
//! §6.2): [`parse_test`] reads the directives, applies the defaults that
//! depend on the input tree, and refuses every test that cannot be judged
//! (each refusal is the `ERROR` outcome of the runner).

use super::tree::{self, Entry, Files};
use std::path::{Path, PathBuf};

/// The expected end of a run (`state` of §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// A `State` of `spec/edits.md` §8.5.
    Verified,
    VerifiedWithResidue,
    NotVerified,
    Incomplete,
    Refused,
    /// The run ends with a usage error (status 2).
    Usage,
    /// The run ends with an input error (status 3).
    Input,
}

impl Expected {
    /// Every value, by its name in `test.txt`.
    pub const ALL: [(&'static str, Expected); 7] = [
        ("verified", Expected::Verified),
        ("verified_with_residue", Expected::VerifiedWithResidue),
        ("not_verified", Expected::NotVerified),
        ("incomplete", Expected::Incomplete),
        ("refused", Expected::Refused),
        ("usage", Expected::Usage),
        ("input", Expected::Input),
    ];

    /// Its name, as in `test.txt` and in the `state` line of a run.
    pub fn name(self) -> &'static str {
        Expected::ALL.iter().find(|(_, e)| *e == self).map_or("", |(n, _)| n)
    }

    /// The default `exit` (§6.2).
    pub fn default_exit(self) -> i32 {
        match self {
            Expected::Verified => 0,
            Expected::Usage => 2,
            Expected::Input => 3,
            Expected::Incomplete => 4,
            _ => 1,
        }
    }

    /// A `State` of a result: the run prints a `result` and a `state` line.
    pub fn is_state(self) -> bool {
        !matches!(self, Expected::Usage | Expected::Input)
    }

    /// A state that may permit `--write` (`spec/edits.md` §8.5); see
    /// [`Test::may_write`].
    pub fn may_write(self) -> bool {
        matches!(self, Expected::Verified | Expected::VerifiedWithResidue)
    }
}

/// An `allow` value: the run is given `--allow-<value>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Allow {
    Residue,
    Unparsed,
    Partial,
    Errors,
}

impl Allow {
    pub const ALL: [(&'static str, Allow); 4] = [
        ("residue", Allow::Residue),
        ("unparsed", Allow::Unparsed),
        ("partial", Allow::Partial),
        ("errors", Allow::Errors),
    ];

    pub fn name(self) -> &'static str {
        Allow::ALL.iter().find(|(_, a)| *a == self).map_or("", |(n, _)| n)
    }
}

/// An oracle of §6.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Oracle {
    Determinism,
    Verify,
    Back,
    Clang,
    Mutation,
}

impl Oracle {
    pub const ALL: [(&'static str, Oracle); 5] = [
        ("determinism", Oracle::Determinism),
        ("verify", Oracle::Verify),
        ("back", Oracle::Back),
        ("clang", Oracle::Clang),
        ("mutation", Oracle::Mutation),
    ];

    pub fn name(self) -> &'static str {
        Oracle::ALL.iter().find(|(_, o)| *o == self).map_or("", |(n, _)| n)
    }
}

/// A host or volume capability a test requires (`spec/harness.md` §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    CaseInsensitive,
}

impl Capability {
    pub const ALL: [(&'static str, Capability); 1] =
        [("case-insensitive", Capability::CaseInsensitive)];

    pub fn name(self) -> &'static str {
        Capability::ALL.iter().find(|(_, c)| *c == self).map_or("", |(n, _)| n)
    }
}

/// A symbolic link the runner creates in the work tree (`spec/harness.md`
/// §6.2): never stored in git.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Symlink {
    /// Path relative to the tree (no leading `/`, no `.` or `..`).
    pub link: String,
    /// Relative target stored in the link, staying inside the tree.
    pub target: String,
}

/// A transform test, `test.txt` parsed with its defaults applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    /// The test's directory, `<…>/<area>/<name>`, as given.
    pub dir: PathBuf,
    /// The transform (`rename`): the directory's parent's name.
    pub area: String,
    /// The directory's name.
    pub name: String,
    /// `spec`: the sections covered, in order; at least one.
    pub spec: Vec<String>,
    /// `run`, split at spaces: the transform command and its own options.
    pub run: Vec<String>,
    pub state: Expected,
    /// `exit`, or the default of `state`.
    pub exit: i32,
    /// `std` without `-std=`: `c++20` by default.
    pub std: String,
    /// `args`, split at spaces.
    pub args: Vec<String>,
    /// The main files relative to the tree, in command order; the default
    /// is every `*.cpp` of `pre/` not under `dep/`, in byte order of paths.
    pub tu: Vec<String>,
    /// Relative to the tree; `.` for the tree itself.
    pub owned: Vec<String>,
    pub readonly: Vec<String>,
    /// Sorted, no two equal.
    pub allow: Vec<Allow>,
    /// The oracles turned off, with their reasons; sorted, no two equal.
    pub oracle_off: Vec<(Oracle, String)>,
    /// `require`: capabilities the test needs; sorted, no two equal.
    pub require: Vec<Capability>,
    /// `symlink`: symbolic links created in the work tree; sorted by `link`.
    pub symlinks: Vec<Symlink>,
    pub xfail: Option<String>,
    /// `cdb.json` exists.
    pub cdb: bool,
    /// `expect.txt` exists.
    pub expect: bool,
    /// `post: none`: the run may write and is expected to write nothing.
    pub post_none: bool,
    /// `timeout`: the seconds each invocation of `slip` (and of the clang
    /// oracle's compiler) may take before its process group is killed.
    pub timeout: u64,
}

/// The default `timeout`, in seconds.
pub const DEFAULT_TIMEOUT: u64 = 600;

impl Test {
    /// `<area>/<name>`.
    pub fn id(&self) -> String {
        format!("{}/{}", self.area, self.name)
    }

    /// Whether `oracle` runs (not turned off by `oracle: no-<name>`).
    pub fn oracle_on(&self, oracle: Oracle) -> bool {
        !self.oracle_off.iter().any(|(o, _)| *o == oracle)
    }

    pub fn allows(&self, allow: Allow) -> bool {
        self.allow.contains(&allow)
    }

    pub fn requires(&self, cap: Capability) -> bool {
        self.require.contains(&cap)
    }

    /// Whether a result of the expected state can permit `--write` under
    /// the test's `allow` lines: `verified`, or `verified_with_residue`
    /// with `allow: residue` or `allow: errors` (a `verified_with_residue`
    /// has at least one reason, and each needs one of them,
    /// `spec/edits.md` §8.5). Whether the run's own result permits it is
    /// `slip_verify::conclude::permits` over its record, after the run.
    pub fn may_write(&self) -> bool {
        match self.state {
            Expected::Verified => true,
            Expected::VerifiedWithResidue => {
                self.allows(Allow::Residue) || self.allows(Allow::Errors)
            }
            _ => false,
        }
    }
}

/// The names a test directory may hold (§6.2).
const ENTRIES: [&str; 5] = ["test.txt", "pre", "post", "expect.txt", "cdb.json"];

/// Options the runner gives every run itself (§6.3 step 2), which `run`
/// may not name.
const RUNNER_OPTIONS: [&str; 13] = [
    "--db",
    "--plan",
    "--owned",
    "--readonly",
    "--store",
    "--relative-to",
    "--write",
    "--patch",
    "-j",
    "--allow-residue",
    "--allow-unparsed",
    "--allow-partial",
    "--allow-errors",
];

/// Reads the test at `dir` (`<…>/<area>/<name>`). `Err` is the reason the
/// test cannot be judged: a malformed `test.txt` (the message names the
/// line, `test.txt:<n>: <what>`), a missing directive, a missing or
/// unreadable `pre/`, a `post/` that is not a set of changed files of
/// `pre/`, an entry of the directory that a test does not have.
pub fn parse_test(dir: &Path) -> Result<Test, String> {
    let (area, name) = area_and_name(dir)?;
    check_entries(dir)?;
    let text = std::fs::read(dir.join("test.txt")).map_err(|e| format!("test.txt: {e}"))?;
    let text = String::from_utf8(text).map_err(|_| "test.txt: not UTF-8".to_owned())?;
    let pre_dir = dir.join("pre");
    if std::fs::symlink_metadata(&pre_dir).is_err() {
        return Err("no pre/ directory".into());
    }
    let pre = tree::read_plain_tree(&pre_dir).map_err(|e| format!("pre/: {e}"))?;

    let mut d = Directives::default();
    for (i, line) in text.split('\n').enumerate() {
        let n = i + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        d.line(n, line, &pre).map_err(|e| format!("test.txt:{n}: {e}"))?;
    }
    let declared_links: std::collections::BTreeSet<&str> =
        d.symlinks.iter().map(|(_, s)| s.link.as_str()).collect();
    for (n, s) in &d.symlinks {
        check_symlink_target(&s.link, &s.target, &declared_links)
            .map_err(|e| format!("test.txt:{n}: {e}"))?;
    }
    let missing_run = d.run.is_none();
    let (Some(run), Some(state)) = (d.run, d.state) else {
        let missing = if missing_run { "run" } else { "state" };
        return Err(format!("test.txt: no `{missing}` directive"));
    };
    if d.spec.is_empty() {
        return Err("test.txt: no `spec` directive".into());
    }
    if run[0] != area {
        return Err(format!(
            "test.txt: `run` is `{}`, which is not the transform of its area `{area}`",
            run[0]
        ));
    }

    let tu = if let Some(tu) = d.tu {
        tu
    } else {
        // `*.cpp` exactly, as §6.2 writes it: the runner judges bytes.
        let tu: Vec<String> = pre
            .iter()
            .filter(|(p, e)| {
                matches!(e, Entry::File { .. })
                    && p.as_bytes().ends_with(b".cpp")
                    && !p.starts_with("dep/")
            })
            .map(|(p, _)| p.clone())
            .collect();
        if tu.is_empty() {
            return Err("pre/ has no *.cpp outside dep/ and `tu` is not given".into());
        }
        tu
    };
    let owned = if d.owned.is_empty() { vec![".".to_owned()] } else { d.owned };
    let readonly = if d.readonly.is_empty() && pre_dir.join("dep").is_dir() {
        vec!["dep".to_owned()]
    } else {
        d.readonly
    };

    let mut allow = d.allow;
    allow.sort();
    let mut oracle_off = d.oracle_off;
    oracle_off.sort();
    let mut require = d.require;
    require.sort();
    let mut symlinks: Vec<Symlink> = d.symlinks.into_iter().map(|(_, s)| s).collect();
    symlinks.sort();
    let parsed = Test {
        dir: dir.to_owned(),
        area,
        name,
        spec: d.spec,
        run,
        state,
        exit: d.exit.unwrap_or_else(|| state.default_exit()),
        std: d.std.unwrap_or_else(|| "c++20".to_owned()),
        args: d.args.unwrap_or_default(),
        tu,
        owned,
        readonly,
        allow,
        oracle_off,
        require,
        symlinks,
        xfail: d.xfail,
        cdb: dir.join("cdb.json").is_file(),
        expect: dir.join("expect.txt").is_file(),
        post_none: d.post_none,
        timeout: d.timeout.unwrap_or(DEFAULT_TIMEOUT),
    };
    check_post(&parsed, &pre)?;
    Ok(parsed)
}

fn area_and_name(dir: &Path) -> Result<(String, String), String> {
    let name = dir.file_name().and_then(|n| n.to_str());
    let area = dir.parent().and_then(Path::file_name).and_then(|n| n.to_str());
    match (area, name) {
        (Some(a), Some(n)) if is_word(a) && is_word(n) => Ok((a.to_owned(), n.to_owned())),
        _ => Err(format!(
            "{}: a test is a directory <area>/<name>, each of ASCII letters, digits, `_`, `-` \
             and `.`",
            dir.display()
        )),
    }
}

fn is_word(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('.')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}

fn check_entries(dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        names.push(entry.file_name());
    }
    names.sort();
    for n in names {
        if !ENTRIES.iter().any(|e| n == *e) {
            return Err(format!(
                "`{}` is not an entry of a test (test.txt, pre/, post/, expect.txt, cdb.json)",
                n.to_string_lossy()
            ));
        }
        // No entry is a symbolic link (a `post/` that is one would lead
        // `--bless` out of the test); `pre/` and `post/` are directories,
        // the others regular files.
        let meta = std::fs::symlink_metadata(dir.join(&n))
            .map_err(|e| format!("{}: {e}", dir.join(&n).display()))?;
        let want_dir = n == "pre" || n == "post";
        let shown = n.to_string_lossy();
        if meta.file_type().is_symlink() {
            return Err(format!("`{shown}` is a symbolic link"));
        }
        if want_dir && !meta.is_dir() {
            return Err(format!("`{shown}` is not a directory"));
        }
        if !want_dir && !meta.is_file() {
            return Err(format!("`{shown}` is not a regular file"));
        }
    }
    if !dir.join("test.txt").is_file() {
        return Err("no test.txt".into());
    }
    Ok(())
}

/// §6.2: "A `post/` file equal to its `pre/` file, or with no `pre/` file,
/// is an error of the test. A test whose state does not permit writing has
/// no `post/`." And a test whose run may write ([`Test::may_write`]) has a
/// `post/` or says `post: none`.
fn check_post(test: &Test, pre: &Files) -> Result<(), String> {
    let post_dir = test.dir.join("post");
    let state = test.state.name();
    if std::fs::symlink_metadata(&post_dir).is_err() {
        if test.may_write() && !test.post_none {
            return Err(format!(
                "the run may write (state `{state}`): the test needs a post/, or `post: none` \
                 if the run writes nothing"
            ));
        }
        if test.post_none && !test.may_write() {
            return Err(format!(
                "`post: none` is for a run that may write, and the state `{state}` with these \
                 `allow` lines cannot"
            ));
        }
        return Ok(());
    }
    if test.post_none {
        return Err("the test has a post/ and says `post: none`".into());
    }
    if !test.may_write() {
        return Err(if test.state.may_write() {
            format!(
                "the state `{state}` permits writing only with `allow: residue` or \
                 `allow: errors`, and the test has a post/"
            )
        } else {
            format!("the state `{state}` does not permit writing, and the test has a post/")
        });
    }
    let post = tree::read_plain_tree(&post_dir).map_err(|e| format!("post/: {e}"))?;
    if !post.values().any(|e| matches!(e, Entry::File { .. })) {
        return Err("post/ holds no file (a run that writes nothing says `post: none`)".into());
    }
    for (path, entry) in &post {
        match (pre.get(path), entry) {
            (_, Entry::Dir) if matches!(pre.get(path), Some(Entry::Dir)) => {}
            (None, _) => return Err(format!("post/{path} has no pre/ file")),
            (Some(p), _) if p == entry => {
                return Err(format!("post/{path} is equal to its pre/ file"));
            }
            (Some(Entry::File { .. }), Entry::File { .. }) => {}
            (Some(_), _) => {
                return Err(format!("post/{path} is not of the kind of its pre/ entry"));
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Directives {
    spec: Vec<String>,
    run: Option<Vec<String>>,
    state: Option<Expected>,
    exit: Option<i32>,
    std: Option<String>,
    args: Option<Vec<String>>,
    tu: Option<Vec<String>>,
    owned: Vec<String>,
    readonly: Vec<String>,
    allow: Vec<Allow>,
    oracle_off: Vec<(Oracle, String)>,
    require: Vec<Capability>,
    symlinks: Vec<(usize, Symlink)>,
    xfail: Option<String>,
    post_none: bool,
    timeout: Option<u64>,
}

fn once<T>(slot: &mut Option<T>, name: &str, value: T) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("`{name}` given twice"));
    }
    *slot = Some(value);
    Ok(())
}

fn words(value: &str) -> Result<Vec<String>, String> {
    let w: Vec<String> = value.split(' ').map(str::to_owned).collect();
    if w.iter().any(String::is_empty) {
        return Err("two spaces in a row: words are separated by one space".into());
    }
    Ok(w)
}

impl Directives {
    #[allow(clippy::too_many_lines, reason = "one arm per directive of spec/harness.md §6.2")]
    fn line(&mut self, line_no: usize, line: &str, pre: &Files) -> Result<(), String> {
        let Some((name, value)) = line.split_once(": ") else {
            return Err(format!("`{line}` is not `<name>: <value>`"));
        };
        if value.is_empty() || value.trim() != value || value.contains('\t') {
            return Err(format!("`{name}`: the value is empty or has white space around it"));
        }
        match name {
            "spec" => {
                if !is_section(value) {
                    return Err(format!(
                        "spec: `{value}` is not a section, as `edits §9.3` (a spec name, a \
                         space, `§`, numbers joined by `.`)"
                    ));
                }
                self.spec.push(value.to_owned());
            }
            "run" => {
                let w = words(value)?;
                if let Some(opt) = w.iter().find(|a| {
                    RUNNER_OPTIONS.iter().any(|o| {
                        a.as_str() == *o
                            || a.strip_prefix(o).is_some_and(|r| r.starts_with('='))
                            || (*o == "-j" && a.starts_with("-j"))
                    })
                }) {
                    return Err(format!("run: `{opt}` is an option the runner gives (§6.3)"));
                }
                once(&mut self.run, "run", w)?;
            }
            "state" => {
                let Some((_, s)) = Expected::ALL.iter().find(|(n, _)| *n == value) else {
                    return Err(format!(
                        "state: `{value}` is not a state (verified, verified_with_residue, \
                         not_verified, incomplete, refused) nor `usage` or `input`"
                    ));
                };
                once(&mut self.state, "state", *s)?;
            }
            "exit" => {
                let ok = !value.is_empty()
                    && value.len() <= 3
                    && value.bytes().all(|b| b.is_ascii_digit());
                let n = if ok { value.parse::<i32>().ok().filter(|n| *n <= 255) } else { None };
                let Some(n) = n else {
                    return Err(format!("exit: `{value}` is not a status 0 to 255"));
                };
                once(&mut self.exit, "exit", n)?;
            }
            "std" => {
                if value.contains(' ') {
                    return Err(format!("std: `{value}` is not one word"));
                }
                once(&mut self.std, "std", value.to_owned())?;
            }
            "args" => {
                let w = words(value)?;
                once(&mut self.args, "args", w)?;
            }
            "tu" => {
                let w = words(value)?;
                for (i, p) in w.iter().enumerate() {
                    check_relative(p, false)?;
                    if !matches!(pre.get(p), Some(Entry::File { .. })) {
                        return Err(format!("tu: `{p}` is not a file of pre/"));
                    }
                    if w[..i].contains(p) {
                        return Err(format!("tu: `{p}` given twice"));
                    }
                }
                once(&mut self.tu, "tu", w)?;
            }
            "owned" | "readonly" => {
                check_relative(value, true)?;
                let exists = value == "." || pre.contains_key(value);
                if !exists {
                    return Err(format!("{name}: `{value}` is not a file or directory of pre/"));
                }
                let list = if name == "owned" { &mut self.owned } else { &mut self.readonly };
                if list.iter().any(|p| p == value) {
                    return Err(format!("{name}: `{value}` given twice"));
                }
                list.push(value.to_owned());
            }
            "allow" => {
                let Some((_, a)) = Allow::ALL.iter().find(|(n, _)| *n == value) else {
                    return Err(format!(
                        "allow: `{value}` is not residue, unparsed, partial or errors"
                    ));
                };
                if self.allow.contains(a) {
                    return Err(format!("allow: `{value}` given twice"));
                }
                self.allow.push(*a);
            }
            "oracle" => {
                let (word, reason) = value.split_once(' ').unwrap_or((value, ""));
                let oracle = word
                    .strip_prefix("no-")
                    .and_then(|o| Oracle::ALL.iter().find(|(n, _)| *n == o))
                    .map(|(_, o)| *o);
                let Some(oracle) = oracle else {
                    return Err(format!(
                        "oracle: `{word}` is not no-determinism, no-verify, no-back, no-clang \
                         or no-mutation"
                    ));
                };
                if reason.trim().is_empty() {
                    return Err(format!("oracle: `{word}` needs a reason"));
                }
                if self.oracle_off.iter().any(|(o, _)| *o == oracle) {
                    return Err(format!("oracle: `{word}` given twice"));
                }
                self.oracle_off.push((oracle, reason.to_owned()));
            }
            "require" => {
                let Some((_, c)) = Capability::ALL.iter().find(|(n, _)| *n == value) else {
                    return Err(format!("require: `{value}` is not `case-insensitive`"));
                };
                if self.require.contains(c) {
                    return Err(format!("require: `{value}` given twice"));
                }
                self.require.push(*c);
            }
            "symlink" => {
                let Some((path, target)) = value.split_once(" -> ") else {
                    return Err(format!("symlink: `{value}` is not `<link> -> <target>`"));
                };
                if path.is_empty()
                    || target.is_empty()
                    || path.contains(' ')
                    || target.contains(' ')
                {
                    return Err(format!("symlink: `{value}` is not `<link> -> <target>`"));
                }
                check_relative(path, false)?;
                if pre.contains_key(path) {
                    return Err(format!("symlink: `{path}` is already in pre/"));
                }
                if self.symlinks.iter().any(|(_, s)| s.link == path) {
                    return Err(format!("symlink: `{path}` given twice"));
                }
                for anc in ancestors(path) {
                    if matches!(pre.get(anc), Some(Entry::File { .. })) {
                        return Err(format!("symlink: `{path}` sits under file `{anc}` of pre/"));
                    }
                    if self.symlinks.iter().any(|(_, s)| s.link == anc) {
                        return Err(format!("symlink: `{path}` sits under symlink `{anc}`"));
                    }
                }
                if self.symlinks.iter().any(|(_, s)| s.link.starts_with(&format!("{path}/"))) {
                    return Err(format!("symlink: `{path}` is a prefix of another symlink"));
                }
                self.symlinks
                    .push((line_no, Symlink { link: path.to_owned(), target: target.to_owned() }));
            }
            "xfail" => once(&mut self.xfail, "xfail", value.to_owned())?,
            "post" => {
                if value != "none" {
                    return Err(format!("post: `{value}` is not `none`"));
                }
                if self.post_none {
                    return Err("`post` given twice".into());
                }
                self.post_none = true;
            }
            "timeout" => {
                let n = (value.len() <= 6 && value.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| value.parse::<u64>().ok())
                    .flatten()
                    .filter(|n| (1..=86_400).contains(n));
                let Some(n) = n else {
                    return Err(format!(
                        "timeout: `{value}` is not a number of seconds 1 to 86400"
                    ));
                };
                once(&mut self.timeout, "timeout", n)?;
            }
            other => return Err(format!("unknown directive `{other}`")),
        }
        Ok(())
    }
}

/// `edits §9.3`: a spec name of lower-case letters, digits and `-`, a
/// space, `§`, and numbers joined by `.`.
fn is_section(value: &str) -> bool {
    let Some((spec, section)) = value.split_once(" §") else { return false };
    let spec_ok = spec.starts_with(|c: char| c.is_ascii_lowercase())
        && spec.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    spec_ok && section.split('.').all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// A path relative to the tree: not absolute, no empty, `.` or `..`
/// component (`.` alone where `dot` allows the tree itself).
fn check_relative(p: &str, dot: bool) -> Result<(), String> {
    if dot && p == "." {
        return Ok(());
    }
    if p.starts_with('/') || p.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        return Err(format!("`{p}` is not a path relative to the tree"));
    }
    Ok(())
}

/// Every strict directory prefix of a relative path `p` (`a/b/c` -> `a`, `a/b`).
fn ancestors(p: &str) -> impl Iterator<Item = &str> {
    p.match_indices('/').map(|(i, _)| &p[..i])
}

/// A `symlink` target: relative, no empty components, resolves inside the
/// tree from `link`'s parent directory, and passes through no declared
/// symbolic link (which could otherwise make a `..` escape the tree on disk).
fn check_symlink_target(
    link: &str,
    target: &str,
    links: &std::collections::BTreeSet<&str>,
) -> Result<(), String> {
    if target.starts_with('/') || target.split('/').any(str::is_empty) {
        return Err(format!("symlink: target `{target}` is not a relative path"));
    }
    let mut parts: Vec<&str> = match link.rsplit_once('/') {
        Some((dir, _)) => dir.split('/').collect(),
        None => Vec::new(),
    };
    for c in target.split('/') {
        match c {
            "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("symlink: target `{target}` escapes the tree"));
                }
            }
            _ => {
                parts.push(c);
                let cur = parts.join("/");
                if links.contains(cur.as_str()) {
                    return Err(format!(
                        "symlink: target `{target}` passes through declared symlink `{cur}`"
                    ));
                }
            }
        }
    }
    if parts.is_empty() {
        return Err(format!("symlink: target `{target}` names the tree root"));
    }
    Ok(())
}
