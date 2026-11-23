//! The command line (`spec/exec.md` §8): arguments to an [`Invocation`].
//! Pure: no file is read and nothing is printed.
//!
//! An option's value is the next argument, or follows `=` for a long option
//! (`--store=x`) and directly for `-j` (`-j4`). An option given twice is a
//! usage error unless it may repeat (`--db`, `--only`, `--root`, `--post`,
//! `--match`). `--` ends the options. Paths and `--only` values are byte
//! strings. `--post-image` is the one option without a value.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

/// The usage text (§8), printed by `--help` and after a usage error. `__worker` and
/// `__panic` are not listed.
pub const USAGE: &str = "\
usage: slip plan  --db [<name>=]<compile_commands.json>...  [--store <dir>] [-j <n>]
       slip parse (--plan <id or file> | --db [<name>=]<compile_commands.json>...)
                  [--store <dir>] [-j <n>] [--only <substring>]... [--limit <n>]
                  [--memory <size>] [--observer-memory <size>] [--observer-timeout <seconds>]
                  [--estimate-base <size>] [--estimate-per-byte <n>]
                  [--worker-jobs <n>] [--worker-stack <size>]
       slip verify --plan <id or file> [--round <n>] [--declared <file>]
                  [--root <dir>]... [--post <dir>]... [--findings <n>] [--locate <n>] [--report <file>]
                  [--store <dir>] [-j <n>] [--only <substring>]... [--limit <n>]
                  [--memory <size>] [--observer-memory <size>] [--observer-timeout <seconds>]
                  [--estimate-base <size>] [--estimate-per-byte <n>]
                  [--worker-jobs <n>] [--worker-stack <size>]
       slip declare --plan <id or file> --from <file> --out <file>
                  [--owned <path>]... [--readonly <path>]...
                  [--root <dir>]... [--post <dir>]... [--store <dir>] [--only <substring>]... [--limit <n>]
       slip units --plan <id or file> [--post-image] [--root <dir>]... [--post <dir>]...
                  [--store <dir>] [--only <substring>]... [--limit <n>] [--match <substring>]...
       slip dump  <file>
       slip resource --write <dir>
       slip --version
       slip --help
<size> is a decimal number with an optional suffix K, M, G or T (powers of 1024).
";

/// The store when `--store` is not given (§2).
pub const DEFAULT_STORE: &str = ".slip";

/// `--worker-jobs` by default (§5.2).
pub const DEFAULT_WORKER_JOBS: u64 = 1;

/// `--worker-stack` by default (§5.2): 1 GiB.
pub const DEFAULT_WORKER_STACK: u64 = 1 << 30;

/// `--findings` by default: the findings a verdict keeps (`spec/verify.md`
/// §5, `limit`).
pub const DEFAULT_FINDINGS: u64 = 64;

/// `--locate` by default: the findings that are named and localised
/// (`spec/verify.md` §5, §6.1).
pub const DEFAULT_LOCATE: u64 = 16;

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// `slip --version`.
    Version,
    /// `slip --help` or `slip -h`: the usage text on standard output.
    Help,
    Plan(PlanArgs),
    Parse(ParseArgs),
    Dump {
        file: PathBuf,
    },
    /// `slip resource --write <dir>` (§8.10): the embedded resource files
    /// written under `dir`.
    Resource {
        write: PathBuf,
    },
    /// `slip verify` (§8.7).
    Verify(VerifyArgs),
    /// `slip declare` (§8.8).
    Declare(DeclareArgs),
    /// `slip units` (§8.9).
    Units(UnitsArgs),
    /// The hidden `slip __worker` (§8.4).
    Worker(WorkerArgs),
    /// The hidden `slip __panic`: panics on purpose, for the test of exit
    /// status 70 (§8.5).
    Panic,
}

/// One `--db` value: `[<name>=]<path>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Db {
    /// The configuration name, when the value has one.
    pub name: Option<String>,
    pub path: PathBuf,
}

/// `slip plan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanArgs {
    /// At least one; several are all named, with distinct names.
    pub dbs: Vec<Db>,
    pub store: PathBuf,
    /// `-j`: at least 1.
    pub jobs: Option<usize>,
}

/// Where `parse` gets its plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanSource {
    /// `--plan <id prefix or file>`, as given.
    Plan(OsString),
    /// `--db …`, with the rules of [`PlanArgs::dbs`].
    Db(Vec<Db>),
}

/// `slip parse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseArgs {
    pub source: PlanSource,
    pub run: RunArgs,
}

/// The options `parse` and `verify` share (§8.2, §8.7): the store, the
/// selection, memory admission, the timeout and the workers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunArgs {
    pub store: PathBuf,
    pub jobs: Option<usize>,
    /// `--only`, in the order given (byte strings).
    pub only: Vec<Vec<u8>>,
    pub limit: Option<u64>,
    /// `--memory`: 0 disables admission.
    pub memory: Option<u64>,
    pub observer_memory: Option<u64>,
    /// `--observer-timeout`, seconds: 0 disables it.
    pub observer_timeout: Option<u64>,
    pub estimate_base: Option<u64>,
    pub estimate_per_byte: Option<u64>,
    /// `--worker-jobs`: at least 1.
    pub worker_jobs: u64,
    /// `--worker-stack`, bytes: at least 1.
    pub worker_stack: u64,
}

/// `slip verify` (§8.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyArgs {
    /// `--plan <id prefix or file>`, as given.
    pub plan: OsString,
    /// `--round` (default 0).
    pub round: u64,
    /// `--declared <file>`: a `DECL` container.
    pub declared: Option<PathBuf>,
    pub image: ImageArgs,
    /// `--findings` (default [`DEFAULT_FINDINGS`]).
    pub findings: u64,
    /// `--locate` (default [`DEFAULT_LOCATE`]).
    pub locate: u64,
    /// `--report <file>`: where the JSON report is written.
    pub report: Option<PathBuf>,
    pub run: RunArgs,
}

/// `slip declare` (§8.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclareArgs {
    pub plan: OsString,
    /// `--from`: the text of `spec/verify.md` §2.3.
    pub from: PathBuf,
    /// `--out`: the `DECL` container written.
    pub out: PathBuf,
    /// `--owned` and `--readonly`: the run's files, as given (made real
    /// paths by the command); `--readonly` only with `--owned`.
    pub owned: Vec<PathBuf>,
    pub readonly: Vec<PathBuf>,
    pub image: ImageArgs,
    pub store: PathBuf,
    pub only: Vec<Vec<u8>>,
    pub limit: Option<u64>,
}

/// `slip units` (§8.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitsArgs {
    pub plan: OsString,
    /// `--post-image`: the post-image's records instead of the pre-image's.
    pub post_image: bool,
    pub image: ImageArgs,
    pub store: PathBuf,
    pub only: Vec<Vec<u8>>,
    pub limit: Option<u64>,
    /// `--match`, in the order given (byte strings).
    pub matches: Vec<Vec<u8>>,
}

/// `slip __worker` (§8.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerArgs {
    pub worker_jobs: u64,
    pub worker_stack: u64,
    /// `--findings`: `limit` of every verify job the worker serves.
    pub findings: u64,
    /// `--locate`: `locate` of every verify job the worker serves.
    pub locate: u64,
}

/// The post-image of the command line (`--root`, `--post`; §8.7): the
/// directories as given, `roots[i]` mapped to `posts[i]`. Equal lengths.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageArgs {
    pub roots: Vec<PathBuf>,
    pub posts: Vec<PathBuf>,
}

/// A usage error (§8.5 status 2): the one line, without the `slip: `
/// prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

fn usage<T>(message: impl Into<String>) -> Result<T, UsageError> {
    Err(UsageError(message.into()))
}

/// `<size>` (§8): decimal digits with an optional suffix `K`, `M`, `G` or
/// `T` (powers of 1024). `None` if malformed or above `u64::MAX`.
pub fn parse_size(text: &str) -> Option<u64> {
    let (digits, shift) = match text.as_bytes().last()? {
        b'K' => (&text[..text.len() - 1], 10),
        b'M' => (&text[..text.len() - 1], 20),
        b'G' => (&text[..text.len() - 1], 30),
        b'T' => (&text[..text.len() - 1], 40),
        _ => (text, 0),
    };
    parse_number(digits)?.checked_mul(1u64 << shift)
}

/// Decimal digits only (no sign, no white space), at most `u64::MAX`.
fn parse_number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Parses the arguments after `argv[0]`.
pub fn parse(args: &[OsString]) -> Result<Invocation, UsageError> {
    let Some((command, rest)) = args.split_first() else {
        return usage("no command given");
    };
    match command.to_str() {
        Some("--version") if rest.is_empty() => Ok(Invocation::Version),
        Some("--version") => usage("`--version` takes no arguments"),
        Some("--help" | "-h") if rest.is_empty() => Ok(Invocation::Help),
        Some("--help" | "-h") => usage("`--help` takes no arguments"),
        Some("plan") => parse_plan(rest),
        Some("parse") => parse_parse(rest),
        Some("verify") => parse_verify(rest),
        Some("declare") => parse_declare(rest),
        Some("units") => parse_units(rest),
        Some("dump") => parse_dump(rest),
        Some("resource") => parse_resource(rest),
        Some("__worker") => parse_worker(rest),
        Some("__panic") if rest.is_empty() => Ok(Invocation::Panic),
        _ => usage(format!("unknown command `{}`", command.to_string_lossy())),
    }
}

// ----- the scanner ------------------------------------------------------------

/// The options of §8 that take no value.
const FLAGS: &[&str] = &["--post-image"];

/// One element of the command line after the command.
enum Item<'a> {
    /// An option and its value (every option of §8 but [`FLAGS`] takes one).
    Option { name: &'static str, value: &'a OsStr, joined: Option<OsString> },
    /// An option of [`FLAGS`].
    Flag(&'static str),
    /// An argument that is not an option.
    Positional(&'a OsStr),
}

/// Splits the arguments into options with their values and positionals.
/// `options` are the names this command accepts (`-j` included where it
/// is accepted).
fn scan<'a>(args: &'a [OsString], options: &[&'static str]) -> Result<Vec<Item<'a>>, UsageError> {
    let mut items = Vec::new();
    let mut i = 0;
    let mut options_ended = false;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        let bytes: &[u8] = &os_bytes(arg);
        if options_ended || bytes.first() != Some(&b'-') || bytes == b"-" {
            items.push(Item::Positional(arg));
            continue;
        }
        if bytes == b"--" {
            options_ended = true;
            continue;
        }
        // `-j4`, `-j 4`.
        if options.contains(&"-j") && bytes.starts_with(b"-j") && !bytes.starts_with(b"--") {
            if bytes.len() > 2 {
                let value = os_from_bytes(&bytes[2..]);
                items.push(Item::Option { name: "-j", value: arg, joined: Some(value) });
            } else {
                let Some(value) = args.get(i) else { return usage("`-j` needs a value") };
                i += 1;
                items.push(Item::Option { name: "-j", value, joined: None });
            }
            continue;
        }
        // `--name=value`, `--name value`.
        let (name_bytes, joined) = match bytes.iter().position(|&b| b == b'=') {
            Some(eq) if bytes.starts_with(b"--") => {
                (&bytes[..eq], Some(os_from_bytes(&bytes[eq + 1..])))
            }
            _ => (bytes, None),
        };
        let Some(&name) =
            options.iter().find(|o| o.as_bytes() == name_bytes && o.starts_with("--"))
        else {
            return usage(format!("unknown option `{}`", arg.to_string_lossy()));
        };
        if FLAGS.contains(&name) {
            if joined.is_some() {
                return usage(format!("`{name}` takes no value"));
            }
            items.push(Item::Flag(name));
        } else if joined.is_some() {
            items.push(Item::Option { name, value: arg, joined });
        } else {
            let Some(value) = args.get(i) else { return usage(format!("`{name}` needs a value")) };
            i += 1;
            items.push(Item::Option { name, value, joined: None });
        }
    }
    Ok(items)
}

impl Item<'_> {
    fn value(&self) -> OsString {
        match self {
            Item::Option { joined: Some(v), .. } => v.clone(),
            Item::Option { value, .. } | Item::Positional(value) => (*value).to_owned(),
            Item::Flag(_) => OsString::new(),
        }
    }
}

/// Collects option values, refusing a single option given twice.
#[derive(Default)]
struct Values {
    seen: Vec<(&'static str, OsString)>,
}

impl Values {
    fn push(
        &mut self,
        name: &'static str,
        value: OsString,
        repeats: bool,
    ) -> Result<(), UsageError> {
        if !repeats && self.seen.iter().any(|(n, _)| *n == name) {
            return usage(format!("`{name}` is given more than once"));
        }
        self.seen.push((name, value));
        Ok(())
    }

    fn all(&self, name: &str) -> impl Iterator<Item = &OsString> {
        self.seen.iter().filter(move |(n, _)| *n == name).map(|(_, v)| v)
    }

    fn one(&self, name: &str) -> Option<&OsString> {
        self.all(name).next()
    }

    fn number(&self, name: &str, min: u64) -> Result<Option<u64>, UsageError> {
        self.one(name)
            .map(|v| match v.to_str().and_then(parse_number) {
                Some(n) if n >= min => Ok(n),
                Some(_) => usage(format!("`{name}` must be at least {min}")),
                None => usage(format!("`{name}`: `{}` is not a number", v.to_string_lossy())),
            })
            .transpose()
    }

    fn size(&self, name: &str, min: u64) -> Result<Option<u64>, UsageError> {
        self.one(name)
            .map(|v| match v.to_str().and_then(parse_size) {
                Some(n) if n >= min => Ok(n),
                Some(_) => usage(format!("`{name}` must be at least {min}")),
                None => usage(format!("`{name}`: `{}` is not a size", v.to_string_lossy())),
            })
            .transpose()
    }

    fn jobs(&self) -> Result<Option<usize>, UsageError> {
        self.number("-j", 1)?
            .map(|n| usize::try_from(n).or_else(|_| usage("`-j` is too large")))
            .transpose()
    }

    fn store(&self) -> PathBuf {
        self.one("--store").map_or_else(|| PathBuf::from(DEFAULT_STORE), PathBuf::from)
    }

    fn paths(&self, name: &str) -> Vec<PathBuf> {
        self.all(name).map(PathBuf::from).collect()
    }

    fn bytes(&self, name: &str) -> Vec<Vec<u8>> {
        self.all(name).map(|v| os_bytes(v).into_owned()).collect()
    }

    /// `--plan`, which `command` requires.
    fn plan(&self, command: &str) -> Result<OsString, UsageError> {
        match self.one("--plan") {
            Some(p) => Ok(p.clone()),
            None => usage(format!("`{command}` needs `--plan`")),
        }
    }

    /// A path option `command` requires.
    fn required_path(&self, command: &str, name: &str) -> Result<PathBuf, UsageError> {
        match self.one(name) {
            Some(p) => Ok(PathBuf::from(p)),
            None => usage(format!("`{command}` needs `{name}`")),
        }
    }

    /// `--root` and `--post` (§8 "Images"): the k-th of each is one root
    /// map, so their numbers must be equal.
    fn image(&self) -> Result<ImageArgs, UsageError> {
        let image = ImageArgs { roots: self.paths("--root"), posts: self.paths("--post") };
        if image.roots.len() != image.posts.len() {
            return usage(format!(
                "`--root` and `--post` come in pairs: {} `--root` and {} `--post` given",
                image.roots.len(),
                image.posts.len()
            ));
        }
        Ok(image)
    }

    /// The options of [`RunArgs`].
    fn run(&self) -> Result<RunArgs, UsageError> {
        Ok(RunArgs {
            store: self.store(),
            jobs: self.jobs()?,
            only: self.bytes("--only"),
            limit: self.number("--limit", 0)?,
            memory: self.size("--memory", 0)?,
            observer_memory: self.size("--observer-memory", 1)?,
            observer_timeout: self.number("--observer-timeout", 0)?,
            estimate_base: self.size("--estimate-base", 0)?,
            estimate_per_byte: self.number("--estimate-per-byte", 0)?,
            worker_jobs: self.number("--worker-jobs", 1)?.unwrap_or(DEFAULT_WORKER_JOBS),
            worker_stack: self.size("--worker-stack", 1)?.unwrap_or(DEFAULT_WORKER_STACK),
        })
    }

    fn dbs(&self) -> Result<Vec<Db>, UsageError> {
        let dbs: Vec<Db> = self.all("--db").map(|v| split_db(v)).collect();
        if dbs.len() > 1 {
            if let Some(unnamed) = dbs.iter().find(|d| d.name.is_none()) {
                return usage(format!(
                    "several databases need names (`--db <name>=<path>`); `{}` has none",
                    unnamed.path.display()
                ));
            }
            for (i, d) in dbs.iter().enumerate() {
                if dbs[..i].iter().any(|e| e.name == d.name) {
                    return usage(format!(
                        "the configuration name `{}` is given more than once",
                        d.name.as_deref().unwrap_or_default()
                    ));
                }
            }
        }
        Ok(dbs)
    }
}

/// Every item an option of `options`; a positional is refused.
fn options_only(
    args: &[OsString],
    options: &[&'static str],
    repeating: &[&str],
) -> Result<Values, UsageError> {
    let mut values = Values::default();
    for item in scan(args, options)? {
        match &item {
            Item::Option { name, .. } | Item::Flag(name) => {
                values.push(name, item.value(), repeating.contains(name))?;
            }
            Item::Positional(p) => {
                return usage(format!("unexpected argument `{}`", p.to_string_lossy()));
            }
        }
    }
    Ok(values)
}

/// A `--db` value split at its first `=` when what precedes it is a
/// non-empty name of ASCII letters, digits, `_`, `-` and `.` (§8).
fn split_db(value: &OsStr) -> Db {
    let bytes = os_bytes(value);
    if let Some(eq) = bytes.iter().position(|&b| b == b'=') {
        let name = &bytes[..eq];
        if !name.is_empty()
            && name.iter().all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return Db {
                name: Some(String::from_utf8_lossy(name).into_owned()),
                path: PathBuf::from(os_from_bytes(&bytes[eq + 1..])),
            };
        }
    }
    Db { name: None, path: PathBuf::from(value) }
}

// ----- the commands -----------------------------------------------------------

fn parse_plan(args: &[OsString]) -> Result<Invocation, UsageError> {
    let values = options_only(args, &["--db", "--store", "-j"], &["--db"])?;
    let dbs = values.dbs()?;
    if dbs.is_empty() {
        return usage("`plan` needs `--db`");
    }
    Ok(Invocation::Plan(PlanArgs { dbs, store: values.store(), jobs: values.jobs()? }))
}

/// The options of [`RunArgs`].
const RUN_OPTIONS: &[&str] = &[
    "--store",
    "-j",
    "--only",
    "--limit",
    "--memory",
    "--observer-memory",
    "--observer-timeout",
    "--estimate-base",
    "--estimate-per-byte",
    "--worker-jobs",
    "--worker-stack",
];

/// `options` followed by [`RUN_OPTIONS`].
fn with_run_options(options: &[&'static str]) -> Vec<&'static str> {
    options.iter().chain(RUN_OPTIONS).copied().collect()
}

fn parse_parse(args: &[OsString]) -> Result<Invocation, UsageError> {
    let values = options_only(args, &with_run_options(&["--plan", "--db"]), &["--db", "--only"])?;
    let dbs = values.dbs()?;
    let source = match (values.one("--plan"), dbs.is_empty()) {
        (Some(_), false) => return usage("`--plan` and `--db` cannot both be given"),
        (None, true) => return usage("`parse` needs `--plan` or `--db`"),
        (Some(plan), true) => PlanSource::Plan(plan.clone()),
        (None, false) => PlanSource::Db(dbs),
    };
    Ok(Invocation::Parse(ParseArgs { source, run: values.run()? }))
}

fn parse_verify(args: &[OsString]) -> Result<Invocation, UsageError> {
    let own = [
        "--plan",
        "--round",
        "--declared",
        "--root",
        "--post",
        "--findings",
        "--locate",
        "--report",
    ];
    let values = options_only(args, &with_run_options(&own), &["--only", "--root", "--post"])?;
    Ok(Invocation::Verify(VerifyArgs {
        plan: values.plan("verify")?,
        round: values.number("--round", 0)?.unwrap_or(0),
        declared: values.one("--declared").map(PathBuf::from),
        image: values.image()?,
        findings: values.number("--findings", 0)?.unwrap_or(DEFAULT_FINDINGS),
        locate: values.number("--locate", 0)?.unwrap_or(DEFAULT_LOCATE),
        report: values.one("--report").map(PathBuf::from),
        run: values.run()?,
    }))
}

fn parse_declare(args: &[OsString]) -> Result<Invocation, UsageError> {
    let values = options_only(
        args,
        &[
            "--plan",
            "--from",
            "--out",
            "--owned",
            "--readonly",
            "--root",
            "--post",
            "--store",
            "--only",
            "--limit",
        ],
        &["--only", "--root", "--post", "--owned", "--readonly"],
    )?;
    let (owned, readonly) = (values.paths("--owned"), values.paths("--readonly"));
    if owned.is_empty() && !readonly.is_empty() {
        return usage("`--readonly` needs `--owned`");
    }
    Ok(Invocation::Declare(DeclareArgs {
        plan: values.plan("declare")?,
        from: values.required_path("declare", "--from")?,
        out: values.required_path("declare", "--out")?,
        owned,
        readonly,
        image: values.image()?,
        store: values.store(),
        only: values.bytes("--only"),
        limit: values.number("--limit", 0)?,
    }))
}

fn parse_units(args: &[OsString]) -> Result<Invocation, UsageError> {
    let values = options_only(
        args,
        &["--plan", "--post-image", "--root", "--post", "--store", "--only", "--limit", "--match"],
        &["--only", "--root", "--post", "--match"],
    )?;
    Ok(Invocation::Units(UnitsArgs {
        plan: values.plan("units")?,
        post_image: values.one("--post-image").is_some(),
        image: values.image()?,
        store: values.store(),
        only: values.bytes("--only"),
        limit: values.number("--limit", 0)?,
        matches: values.bytes("--match"),
    }))
}

fn parse_dump(args: &[OsString]) -> Result<Invocation, UsageError> {
    let mut files = Vec::new();
    for item in scan(args, &[])? {
        match item {
            Item::Positional(p) => files.push(p),
            Item::Option { .. } | Item::Flag(_) => unreachable!("`dump` accepts no option"),
        }
    }
    match files.as_slice() {
        [file] => Ok(Invocation::Dump { file: PathBuf::from(file) }),
        [] => usage("`dump` needs a file"),
        _ => usage("`dump` takes one file"),
    }
}

fn parse_resource(args: &[OsString]) -> Result<Invocation, UsageError> {
    let values = options_only(args, &["--write"], &[])?;
    match values.one("--write") {
        Some(dir) => Ok(Invocation::Resource { write: PathBuf::from(dir) }),
        None => usage("`resource` needs `--write`"),
    }
}

fn parse_worker(args: &[OsString]) -> Result<Invocation, UsageError> {
    let values =
        options_only(args, &["--worker-jobs", "--worker-stack", "--findings", "--locate"], &[])?;
    Ok(Invocation::Worker(WorkerArgs {
        worker_jobs: values.number("--worker-jobs", 1)?.unwrap_or(DEFAULT_WORKER_JOBS),
        worker_stack: values.size("--worker-stack", 1)?.unwrap_or(DEFAULT_WORKER_STACK),
        findings: values.number("--findings", 0)?.unwrap_or(DEFAULT_FINDINGS),
        locate: values.number("--locate", 0)?.unwrap_or(DEFAULT_LOCATE),
    }))
}

// ----- byte strings -------------------------------------------------------------

#[cfg(unix)]
pub(crate) fn os_bytes(s: &OsStr) -> std::borrow::Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    std::borrow::Cow::Borrowed(s.as_bytes())
}

#[cfg(unix)]
pub(crate) fn os_from_bytes(b: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStrExt;
    OsStr::from_bytes(b).to_owned()
}

#[cfg(not(unix))]
pub(crate) fn os_bytes(s: &OsStr) -> std::borrow::Cow<'_, [u8]> {
    match s.to_string_lossy() {
        std::borrow::Cow::Borrowed(t) => std::borrow::Cow::Borrowed(t.as_bytes()),
        std::borrow::Cow::Owned(t) => std::borrow::Cow::Owned(t.into_bytes()),
    }
}

#[cfg(not(unix))]
pub(crate) fn os_from_bytes(b: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(b).into_owned())
}
