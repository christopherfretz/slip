//! `cargo xtask corpus` (`spec/harness.md` §8): fetch a pinned corpus, configure it
//! so it has a compilation database, and build the targets that generate
//! the sources its translation units include.
//!
//! ```text
//! cargo xtask corpus list
//! cargo xtask corpus fetch <name>
//! cargo xtask corpus configure <name>
//! cargo xtask corpus status <name>
//! cargo xtask corpus path <name> [--db]
//! ```
//!
//! A corpus lives in `<root>/<name>/`: `src/` (the clone), `build/` (the
//! cmake build tree), the stamps `fetch.stamp`, `configure.stamp` and
//! `generate.stamp`, the logs `fetch.log` (in `<root>/<name>/`),
//! `build/configure.log` and `build/generate.log`, and `lock` while a step
//! runs. `<root>` is `$SLIP_CORPUS_DIR`, else `.corpus/` in the repository.
//!
//! Each step writes its stamp only after it succeeded, and does nothing
//! when its stamp already holds what it would write: the pin for `fetch`,
//! the commit and every cmake argument for `configure`, and that plus the
//! target list for the generate step. A step that runs first removes the
//! stamps of the steps after it.

pub mod build_tree;
pub mod rows;

pub use rows::Corpus;

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

const USAGE: &str = "\
usage: cargo xtask corpus list
       cargo xtask corpus fetch <name>
       cargo xtask corpus configure <name>
       cargo xtask corpus status <name>
       cargo xtask corpus path <name> [--db]";

/// What a missing tool's message tells the user to run.
pub const PROVISION: &str = "scripts/provision.sh --full";

/// The first line of each stamp; a stamp that does not start with it is
/// not current.
const STAMP_VERSION: &str = "slip-corpus-stamp 1";

/// The corpus root: `env` (the value of `$SLIP_CORPUS_DIR`) if set and
/// non-empty, else `.corpus` in the repository `repo`. A relative
/// `$SLIP_CORPUS_DIR` is an error, so the root does not depend on the
/// directory xtask runs in.
pub fn corpus_root(repo: &Path, env: Option<&OsStr>) -> Result<PathBuf, String> {
    match env.filter(|v| !v.is_empty()) {
        None => Ok(repo.join(".corpus")),
        Some(v) if Path::new(v).is_absolute() => Ok(PathBuf::from(v)),
        Some(v) => Err(format!("SLIP_CORPUS_DIR must be an absolute path, not `{}`", v.display())),
    }
}

/// The files of one corpus under the corpus root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// `<root>/<name>`.
    pub dir: PathBuf,
    /// `<root>/<name>/src`: the clone.
    pub src: PathBuf,
    /// `<root>/<name>/build`: the build tree.
    pub build: PathBuf,
}

impl Layout {
    pub fn new(root: &Path, name: &str) -> Layout {
        let dir = root.join(name);
        Layout { src: dir.join("src"), build: dir.join("build"), dir }
    }
    /// The cmake source directory: `src`, or its `source_subdir`.
    pub fn source_dir(&self, c: &Corpus) -> PathBuf {
        c.source_subdir.as_ref().map_or(self.src.clone(), |s| self.src.join(s))
    }
    /// The compilation database.
    pub fn database(&self) -> PathBuf {
        self.build.join("compile_commands.json")
    }
    pub fn fetch_stamp(&self) -> PathBuf {
        self.dir.join("fetch.stamp")
    }
    pub fn configure_stamp(&self) -> PathBuf {
        self.dir.join("configure.stamp")
    }
    pub fn generate_stamp(&self) -> PathBuf {
        self.dir.join("generate.stamp")
    }
    pub fn lock(&self) -> PathBuf {
        self.dir.join("lock")
    }
    /// The partial clone, renamed to `src` once the tag is verified.
    fn partial(&self) -> PathBuf {
        self.dir.join("src.partial")
    }
}

/// The message for a tool that is not on `PATH`.
pub fn missing_tool(name: &str) -> String {
    format!("`{name}` is not on PATH: run {PROVISION} (nothing is installed by xtask)")
}

/// The first executable file `name` in the directories of `path` (a
/// `PATH` value); `Err` is [`missing_tool`].
pub fn find_tool(name: &str, path: Option<&OsStr>) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    for dir in std::env::split_paths(path.unwrap_or_default()) {
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        let p = dir.join(name);
        if let Ok(m) = std::fs::metadata(&p)
            && m.is_file()
            && m.permissions().mode() & 0o111 != 0
        {
            return Ok(p);
        }
    }
    Err(missing_tool(name))
}

/// Where a command runs: the corpus files, the `PATH` tools are looked up
/// in, and the writer for progress lines.
pub struct Ctx<'a> {
    pub layout: Layout,
    pub path: Option<OsString>,
    pub out: &'a mut dyn Write,
}

impl std::fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx").field("layout", &self.layout).field("path", &self.path).finish()
    }
}

impl Ctx<'_> {
    fn say(&mut self, line: &str) {
        let _ = writeln!(self.out, "{line}");
        let _ = self.out.flush();
    }
    fn tool(&self, name: &str) -> Result<PathBuf, String> {
        find_tool(name, self.path.as_deref())
    }
}

/// Whether a step ran or found its stamp current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Done {
    Ran,
    Current,
}

/// The fetch stamp for `c` at `commit`.
pub fn fetch_stamp(c: &Corpus, commit: &str) -> String {
    format!("{STAMP_VERSION}\nstep fetch\nurl {}\ntag {}\ncommit {commit}\n", c.url, c.tag)
}

/// The cmake arguments of `configure` for `c`, with the compilers
/// resolved to `cc` and `cxx`.
pub fn cmake_args(c: &Corpus, layout: &Layout, cc: &Path, cxx: &Path) -> Vec<String> {
    let mut a = vec![
        "--fresh".to_owned(),
        "-S".to_owned(),
        layout.source_dir(c).display().to_string(),
        "-B".to_owned(),
        layout.build.display().to_string(),
        "-G".to_owned(),
        "Unix Makefiles".to_owned(),
        format!("-DCMAKE_BUILD_TYPE={}", c.build_type),
        format!("-DCMAKE_C_COMPILER={}", cc.display()),
        format!("-DCMAKE_CXX_COMPILER={}", cxx.display()),
        format!("-DCMAKE_CXX_STANDARD={}", c.cxx_std),
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON".to_owned(),
    ];
    a.extend(c.configure.iter().cloned());
    a
}

/// The configure stamp: the commit and every cmake argument.
pub fn configure_stamp(commit: &str, args: &[String]) -> String {
    let mut s = format!("{STAMP_VERSION}\nstep configure\ncommit {commit}\n");
    for a in args {
        let _ = writeln!(s, "arg {a}");
    }
    s
}

/// The generate stamp: the configure stamp it builds on and the targets.
pub fn generate_stamp(configure: &str, targets: &[String]) -> String {
    let mut s = format!("{STAMP_VERSION}\nstep generate\n");
    for line in configure.lines().skip(1) {
        let _ = writeln!(s, "configured {line}");
    }
    for t in targets {
        let _ = writeln!(s, "target {t}");
    }
    s
}

fn stamp_is(path: &Path, want: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|have| have == want)
}

fn write_stamp(path: &Path, text: &str) -> Result<(), String> {
    // Written beside and renamed, so a stamp is never half written.
    let tmp = path.with_extension("stamp.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn remove_stamp(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove {}: {e}", path.display())),
    }
}

/// The commit recorded by a current fetch stamp.
fn stamped_commit(c: &Corpus, layout: &Layout) -> Option<String> {
    let text = std::fs::read_to_string(layout.fetch_stamp()).ok()?;
    let commit = text.lines().find_map(|l| l.strip_prefix("commit "))?.to_owned();
    (text == fetch_stamp(c, &commit)).then_some(commit)
}

/// Holds `<root>/<name>/lock` while a step runs; removed on drop.
#[derive(Debug)]
struct Lock(PathBuf);

impl Lock {
    /// Takes the lock, or fails naming the process that holds it. A lock
    /// whose process no longer exists (`/proc/<pid>`) is stale and taken
    /// over; one whose holder cannot be checked is not.
    fn take(layout: &Layout) -> Result<Lock, String> {
        std::fs::create_dir_all(&layout.dir)
            .map_err(|e| format!("cannot create {}: {e}", layout.dir.display()))?;
        let path = layout.lock();
        for _ in 0..2 {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let _ = writeln!(f, "{}", std::process::id());
                    return Ok(Lock(path));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let holder = std::fs::read_to_string(&path).unwrap_or_default();
                    let pid = holder.trim();
                    let alive = pid.is_empty()
                        || !pid.bytes().all(|b| b.is_ascii_digit())
                        || !Path::new("/proc/self").exists()
                        || Path::new("/proc").join(pid).exists();
                    if alive {
                        return Err(format!(
                            "{} is held by process {} (another `corpus` command); \
                             wait for it, or remove the file if that process is gone",
                            path.display(),
                            if pid.is_empty() { "?" } else { pid }
                        ));
                    }
                    remove_stamp(&path)?;
                }
                Err(e) => return Err(format!("cannot create {}: {e}", path.display())),
            }
        }
        Err(format!("cannot take {}", path.display()))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Runs `cmd` with its output appended to the file `log`; `Err` names the
/// command, its status and the log, after printing the log's last lines.
fn run_logged(cmd: &mut Command, what: &str, log: &Path, ctx: &mut Ctx) -> Result<(), String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("cannot open {}: {e}", log.display()))?;
    let err = file.try_clone().map_err(|e| format!("cannot open {}: {e}", log.display()))?;
    let status = cmd
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(err)
        .status()
        .map_err(|e| format!("cannot run {what}: {e}"))?;
    if status.success() {
        return Ok(());
    }
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    for l in &lines[lines.len().saturating_sub(20)..] {
        ctx.say(&format!("  | {l}"));
    }
    Err(format!("{what} failed ({status}); the log is {}", log.display()))
}

/// Runs `git` with `args` in `dir` and returns its trimmed stdout.
fn git_output(git: &Path, dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(git)
        .args(["-c", "safe.directory=*"])
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The commit checked out in `dir`, after checking it is the commit of
/// tag `tag` there.
fn verify_tag(git: &Path, dir: &Path, tag: &str) -> Result<String, String> {
    let head = git_output(git, dir, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let tagged =
        git_output(git, dir, &["rev-parse", "--verify", &format!("refs/tags/{tag}^{{commit}}")])
            .map_err(|_| format!("{} has no tag `{tag}`", dir.display()))?;
    if head != tagged {
        return Err(format!("{} is at {head}, not at tag `{tag}` ({tagged})", dir.display()));
    }
    Ok(head)
}

/// `corpus fetch`: a shallow clone of the pinned tag into `src`.
pub fn fetch(c: &Corpus, ctx: &mut Ctx) -> Result<Done, String> {
    let _lock = Lock::take(&ctx.layout)?;
    fetch_locked(c, ctx)
}

fn fetch_locked(c: &Corpus, ctx: &mut Ctx) -> Result<Done, String> {
    let layout = ctx.layout.clone();
    if layout.src.exists() && stamped_commit(c, &layout).is_some() {
        ctx.say(&format!("fetch: {} {} is present", c.name, c.tag));
        return Ok(Done::Current);
    }
    let git = ctx.tool("git")?;
    if layout.src.exists() {
        // A clone without a current stamp: keep it only if it is the pin.
        let commit = verify_tag(&git, &layout.src, &c.tag).map_err(|e| {
            format!(
                "{e}; {} does not hold the pin of [{}] ({} {}): remove it to fetch again",
                layout.src.display(),
                c.name,
                c.url,
                c.tag
            )
        })?;
        for s in [layout.configure_stamp(), layout.generate_stamp()] {
            remove_stamp(&s)?;
        }
        write_stamp(&layout.fetch_stamp(), &fetch_stamp(c, &commit))?;
        ctx.say(&format!("fetch: {} is at {} ({commit}); stamped", layout.src.display(), c.tag));
        return Ok(Done::Ran);
    }
    for s in [layout.fetch_stamp(), layout.configure_stamp(), layout.generate_stamp()] {
        remove_stamp(&s)?;
    }
    let partial = layout.partial();
    if partial.exists() {
        std::fs::remove_dir_all(&partial)
            .map_err(|e| format!("cannot remove the partial clone {}: {e}", partial.display()))?;
    }
    ctx.say(&format!("fetch: cloning {} at {} into {}", c.url, c.tag, layout.src.display()));
    let start = Instant::now();
    let log = layout.dir.join("fetch.log");
    let _ = std::fs::remove_file(&log);
    run_logged(
        Command::new(&git)
            .current_dir(&layout.dir)
            .args([
                "-c",
                "advice.detachedHead=false",
                "clone",
                "--quiet",
                "--depth",
                "1",
                "--branch",
                &c.tag,
                "--",
                &c.url,
            ])
            .arg(&partial),
        "git clone",
        &log,
        ctx,
    )?;
    let commit = verify_tag(&git, &partial, &c.tag)?;
    std::fs::rename(&partial, &layout.src)
        .map_err(|e| format!("cannot rename {} to src: {e}", partial.display()))?;
    write_stamp(&layout.fetch_stamp(), &fetch_stamp(c, &commit))?;
    ctx.say(&format!(
        "fetch: {} {} ({commit}) in {:.1}s",
        c.name,
        c.tag,
        start.elapsed().as_secs_f64()
    ));
    Ok(Done::Ran)
}

/// What `configure` did and found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configured {
    pub cmake: Done,
    pub generate: Done,
    /// The entries of the compilation database.
    pub entries: usize,
    /// The generated files the targets promise, all present.
    pub generated: usize,
}

/// `corpus configure`: fetch (a no-op when present), cmake with
/// `CMAKE_EXPORT_COMPILE_COMMANDS=ON`, then `make` of the generate
/// targets, then the generated check.
pub fn configure(c: &Corpus, ctx: &mut Ctx) -> Result<Configured, String> {
    let _lock = Lock::take(&ctx.layout)?;
    fetch_locked(c, ctx)?;
    let layout = ctx.layout.clone();
    let Some(commit) = stamped_commit(c, &layout) else {
        unreachable!("fetch_locked succeeded but left no current fetch stamp");
    };
    let cmake = ctx.tool("cmake")?;
    let make = ctx.tool("make")?;
    let cc = ctx.tool(&c.c_compiler)?;
    let cxx = ctx.tool(&c.cxx_compiler)?;
    let args = cmake_args(c, &layout, &cc, &cxx);
    let cstamp = configure_stamp(&commit, &args);
    let cmake_done = if stamp_is(&layout.configure_stamp(), &cstamp) && layout.database().is_file()
    {
        ctx.say(&format!("configure: {} is configured", layout.build.display()));
        Done::Current
    } else {
        remove_stamp(&layout.configure_stamp())?;
        remove_stamp(&layout.generate_stamp())?;
        std::fs::create_dir_all(&layout.build)
            .map_err(|e| format!("cannot create {}: {e}", layout.build.display()))?;
        let log = layout.build.join("configure.log");
        let _ = std::fs::remove_file(&log);
        ctx.say(&format!(
            "configure: cmake {} (log {})",
            layout.source_dir(c).display(),
            log.display()
        ));
        let start = Instant::now();
        run_logged(
            Command::new(&cmake).args(&args).current_dir(&layout.build),
            "cmake",
            &log,
            ctx,
        )?;
        if !layout.database().is_file() {
            return Err(format!("cmake wrote no {}", layout.database().display()));
        }
        write_stamp(&layout.configure_stamp(), &cstamp)?;
        ctx.say(&format!("configure: cmake done in {:.1}s", start.elapsed().as_secs_f64()));
        Done::Ran
    };
    let gstamp = generate_stamp(&cstamp, &c.generate);
    // Before building: a listed target the build does not have is an
    // error, not a make failure deep in the log.
    let plan = build_tree::plan(&layout.build, &c.generate)?;
    let promised = &plan.promised;
    let generate_done = if stamp_is(&layout.generate_stamp(), &gstamp) {
        ctx.say(&format!("generate: {} targets are built", c.generate.len()));
        Done::Current
    } else {
        remove_stamp(&layout.generate_stamp())?;
        run_generate(&plan, &make, ctx)?;
        Done::Ran
    };
    let missing: Vec<&PathBuf> = promised.iter().filter(|p| !p.is_file()).collect();
    if !missing.is_empty() {
        remove_stamp(&layout.generate_stamp())?;
        for p in missing.iter().take(20) {
            ctx.say(&format!("  missing {}", p.display()));
        }
        return Err(format!(
            "{} of the {} generated files the generate targets promise are missing",
            missing.len(),
            promised.len()
        ));
    }
    if generate_done == Done::Ran {
        write_stamp(&layout.generate_stamp(), &gstamp)?;
    }
    let entries = database_entries(&layout.database())?;
    ctx.say(&format!(
        "configure: {} generated files present; {} has {entries} entries",
        promised.len(),
        layout.database().display()
    ));
    Ok(Configured {
        cmake: cmake_done,
        generate: generate_done,
        entries,
        generated: promised.len(),
    })
}

/// The generate step's `make` runs, logged to `build/generate.log`.
fn run_generate(plan: &build_tree::Plan, make: &Path, ctx: &mut Ctx) -> Result<(), String> {
    let layout = ctx.layout.clone();
    let log = layout.build.join("generate.log");
    let _ = std::fs::remove_file(&log);
    let jobserver = std::env::var("MAKEFLAGS")
        .ok()
        .filter(|m| m.contains("--jobserver-auth="))
        .map_or("no jobserver in MAKEFLAGS: make runs one job".to_owned(), |_| {
            "jobs from the MAKEFLAGS jobserver".to_owned()
        });
    ctx.say(&format!(
        "generate: make {} targets ({jobserver}; log {})",
        plan.targets.len() + plan.files.len(),
        log.display()
    ));
    let start = Instant::now();
    // No `-j`: make takes its parallelism from MAKEFLAGS.
    if !plan.targets.is_empty() {
        run_logged(
            Command::new(make).args(&plan.targets).current_dir(&layout.build),
            "make",
            &log,
            ctx,
        )?;
    }
    for (makefile, file) in &plan.files {
        run_logged(
            Command::new(make).args(["-f", makefile, file]).current_dir(&layout.build),
            "make",
            &log,
            ctx,
        )?;
    }
    ctx.say(&format!("generate: done in {:.1}s", start.elapsed().as_secs_f64()));
    Ok(())
}

fn database_entries(path: &Path) -> Result<usize, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    build_tree::database_entries(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// What is present of one corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The commit of a clone with a current fetch stamp.
    pub source: Option<String>,
    /// The build tree exists.
    pub build: bool,
    /// A configure stamp exists for the current commit; whether its
    /// configuration is the row's is only known with the compilers
    /// resolved, so `status` reports the stamp, not a comparison.
    pub configured: bool,
    /// The database's entries, if it exists and reads.
    pub database: Option<Result<usize, String>>,
    /// The generate stamp is current for the configure stamp and the
    /// row's targets.
    pub generated: bool,
}

/// `corpus status`: reads the files, runs nothing.
pub fn status(c: &Corpus, layout: &Layout) -> Status {
    let source = if layout.src.exists() { stamped_commit(c, layout) } else { None };
    let cstamp = std::fs::read_to_string(layout.configure_stamp()).ok();
    let configured = match (&source, &cstamp) {
        (Some(commit), Some(s)) => {
            s.starts_with(&format!("{STAMP_VERSION}\nstep configure\ncommit {commit}\n"))
        }
        _ => false,
    };
    let database = layout.database().is_file().then(|| database_entries(&layout.database()));
    let generated = configured
        && cstamp
            .as_deref()
            .is_some_and(|s| stamp_is(&layout.generate_stamp(), &generate_stamp(s, &c.generate)));
    Status { source, build: layout.build.is_dir(), configured, database, generated }
}

/// The lines `corpus status` prints.
pub fn render_status(c: &Corpus, layout: &Layout, s: &Status) -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "corpus    {} ({} {}, [{}] of {})",
        c.name,
        c.url,
        c.tag,
        c.kiln,
        rows::KILN_PROJECTS
    );
    let _ = writeln!(o, "directory {}", layout.dir.display());
    let _ = writeln!(
        o,
        "source    {}",
        match &s.source {
            Some(commit) => format!("present, {} at {commit}", c.tag),
            None if layout.src.exists() =>
                "present, not stamped for this pin: run `corpus fetch`".to_owned(),
            None => "absent".to_owned(),
        }
    );
    let _ = writeln!(
        o,
        "build     {}",
        match (s.build, s.configured) {
            (true, true) => "configured",
            (true, false) => "present, not configured for this source",
            (false, _) => "absent",
        }
    );
    let _ = writeln!(
        o,
        "database  {}",
        match &s.database {
            Some(Ok(n)) => format!("{} ({n} entries)", layout.database().display()),
            Some(Err(e)) => format!("unreadable: {e}"),
            None => "absent".to_owned(),
        }
    );
    let _ = writeln!(
        o,
        "generated {}",
        if s.generated {
            format!("done ({} targets)", c.generate.len())
        } else {
            "not done".to_owned()
        }
    );
    o
}

/// `corpus list`: one line per corpus.
pub fn render_list(corpora: &[Corpus]) -> String {
    let rows: Vec<[String; 4]> = corpora
        .iter()
        .map(|c| [c.name.clone(), c.kiln.clone(), c.tag.clone(), c.url.clone()])
        .collect();
    let header = ["name", "kiln", "tag", "url"].map(str::to_owned);
    let mut width = [0usize; 4];
    for r in std::iter::once(&header).chain(&rows) {
        for (w, cell) in width.iter_mut().zip(r) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut o = String::new();
    for r in std::iter::once(&header).chain(&rows) {
        let mut line = String::new();
        for (i, cell) in r.iter().enumerate() {
            let _ = write!(line, "{cell:<w$}  ", w = width[i]);
        }
        let _ = writeln!(o, "{}", line.trim_end());
    }
    o
}

/// `cargo xtask corpus …` for the repository at `repo`.
pub fn main(repo: &Path, args: &[String]) -> ExitCode {
    match run(repo, args, std::env::var_os("SLIP_CORPUS_DIR").as_deref(), std::env::var_os("PATH"))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(e)) => {
            eprintln!("xtask corpus: {e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(e)) => {
            eprintln!("xtask corpus: {e}");
            ExitCode::from(1)
        }
    }
}

enum Failure {
    Usage(String),
    Error(String),
}

fn run(
    repo: &Path,
    args: &[String],
    root: Option<&OsStr>,
    path: Option<OsString>,
) -> Result<(), Failure> {
    let usage = |s: &str| Failure::Usage(s.to_owned());
    let cmd = args.first().map(String::as_str).ok_or_else(|| usage("missing command"))?;
    let rest = &args[1..];
    let (name, db) = match (cmd, rest) {
        ("list", []) => (None, false),
        ("fetch" | "configure" | "status" | "path", [n]) => (Some(n.as_str()), false),
        ("path", [n, f]) if f == "--db" => (Some(n.as_str()), true),
        ("list" | "fetch" | "configure" | "status" | "path", _) => {
            return Err(usage(&format!("wrong arguments for `{cmd}`")));
        }
        (other, _) => return Err(usage(&format!("unknown command `{other}`"))),
    };
    let corpora = rows::load(repo).map_err(Failure::Error)?;
    let Some(name) = name else {
        print!("{}", render_list(&corpora));
        return Ok(());
    };
    let c = rows::find(&corpora, name).map_err(Failure::Error)?;
    let root = corpus_root(repo, root).map_err(Failure::Error)?;
    let layout = Layout::new(&root, &c.name);
    let mut stdout = std::io::stdout();
    let mut ctx = Ctx { layout: layout.clone(), path, out: &mut stdout };
    match cmd {
        "fetch" => fetch(c, &mut ctx).map(|_| ()).map_err(Failure::Error),
        "configure" => configure(c, &mut ctx).map(|_| ()).map_err(Failure::Error),
        "status" => {
            print!("{}", render_status(c, &layout, &status(c, &layout)));
            Ok(())
        }
        "path" if db => {
            if layout.database().is_file() {
                println!("{}", layout.database().display());
                Ok(())
            } else {
                Err(Failure::Error(format!(
                    "{} does not exist: run `cargo xtask corpus configure {}`",
                    layout.database().display(),
                    c.name
                )))
            }
        }
        "path" => {
            println!("{}", layout.dir.display());
            Ok(())
        }
        _ => unreachable!("the argument match admits only the five commands, not `{cmd}`"),
    }
}
