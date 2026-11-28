//! `bench/corpora.toml` and the Kiln row each corpus names
//! (`spec/harness.md` §8.1).
//!
//! A corpus is one table of `bench/corpora.toml`. Its `kiln` key names a
//! table of `kiln/scripts/oss/projects.toml`, from which the URL, the tag
//! and the cmake source directory are taken, so Slip and Kiln measure the
//! same code; everything about how Slip configures it is in Slip's row.
//! The file is read with the line reader of [`crate::lint::manifest`]: it
//! is TOML in the subset that reader understands (tables, strings,
//! integers, arrays of strings), and anything else is rejected.

use crate::lint::manifest::{self, Entry};
use slip_model::{Reason, Status};
use std::collections::BTreeMap;
use std::path::Path;

/// Slip's list of corpora, relative to the repository root.
pub const CORPORA: &str = "bench/corpora.toml";
/// Kiln's list of open-source projects, relative to the repository root.
pub const KILN_PROJECTS: &str = "kiln/scripts/oss/projects.toml";

/// The keys a corpus row may have; all except `expected_unparsed` and
/// `expected_excluded` are required.
const KEYS: &[&str] = &[
    "kiln",
    "cxx_std",
    "build_type",
    "c_compiler",
    "cxx_compiler",
    "configure",
    "generate",
    "expected_unparsed",
    "expected_excluded",
];

/// One observer of a corpus that `corpus rename`'s `coverage` check expects
/// to end with a non-`ok` map outcome (`spec/harness.md` §6.5, §8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedUnparsed {
    /// Path relative to the corpus's `src/`.
    pub path: String,
    /// Expected map shard status (`diagnosed`, `rejected`, or `failed`).
    pub status: Status,
    /// One-line reason.
    pub reason: String,
}

/// One database entry of a corpus that `corpus rename`'s `coverage` check
/// expects the plan to exclude (`spec/harness.md` §6.5, §8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedExcluded {
    /// Path relative to the corpus's `src/`.
    pub path: String,
    /// Expected plan exclusion reason (`spec/model.md` §7).
    pub reason: Reason,
    /// One-line note.
    pub note: String,
}

/// The cmake variables Slip sets itself, which `configure` must not set.
const RESERVED: &[&str] = &[
    "CMAKE_EXPORT_COMPILE_COMMANDS",
    "CMAKE_BUILD_TYPE",
    "CMAKE_C_COMPILER",
    "CMAKE_CXX_COMPILER",
    "CMAKE_CXX_STANDARD",
];

/// One corpus: its row of `bench/corpora.toml` with the pin from Kiln's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    /// The table name (`llvm`), also its directory under the corpus root.
    pub name: String,
    /// The table of `kiln/scripts/oss/projects.toml` it names.
    pub kiln: String,
    /// From the Kiln row: the git URL.
    pub url: String,
    /// From the Kiln row: the pinned tag.
    pub tag: String,
    /// From the Kiln row: the cmake source directory inside the checkout
    /// (`None`: the checkout itself).
    pub source_subdir: Option<String>,
    pub cxx_std: u32,
    pub build_type: String,
    /// The C and C++ compilers, as names looked up on `PATH`.
    pub c_compiler: String,
    pub cxx_compiler: String,
    /// Extra cmake arguments, in order.
    pub configure: Vec<String>,
    /// The build targets that produce generated sources, in order.
    pub generate: Vec<String>,
    /// Observers expected to end with a non-`ok` map outcome
    /// (`spec/harness.md` §6.5, §8.1).
    pub expected_unparsed: Vec<ExpectedUnparsed>,
    /// Database entries expected to be excluded by the plan
    /// (`spec/harness.md` §6.5, §8.1).
    pub expected_excluded: Vec<ExpectedExcluded>,
}

/// Reads `bench/corpora.toml` and `kiln/scripts/oss/projects.toml` under
/// `repo`.
pub fn load(repo: &Path) -> Result<Vec<Corpus>, String> {
    let read = |rel: &str| {
        std::fs::read_to_string(repo.join(rel)).map_err(|e| format!("cannot read {rel}: {e}"))
    };
    parse(&read(CORPORA)?, &read(KILN_PROJECTS)?)
}

/// The corpora of `corpora` (the text of `bench/corpora.toml`), with
/// their pins taken from `kiln` (the text of Kiln's `projects.toml`).
pub fn parse(corpora: &str, kiln: &str) -> Result<Vec<Corpus>, String> {
    let rows = tables(corpora, CORPORA)?;
    let kiln_rows = tables(kiln, KILN_PROJECTS)?;
    let mut out = Vec::new();
    for (name, (line, entries)) in rows {
        let at = |l: usize| format!("{CORPORA}:{l}");
        if !valid_name(&name) {
            return Err(format!(
                "{}: corpus name `{name}` must be lower-case letters, digits, `-` and `_`",
                at(line)
            ));
        }
        for e in entries.values() {
            if !KEYS.contains(&e.key.as_str()) {
                return Err(format!("{}: unknown key `{}` in [{name}]", at(e.line), e.key));
            }
        }
        let get = |key: &str| {
            entries.get(key).ok_or_else(|| format!("{}: [{name}] has no `{key}`", at(line)))
        };
        let kiln_name = string(get("kiln")?, CORPORA)?;
        let cxx_std = integer(get("cxx_std")?, CORPORA)?;
        let build_type = string(get("build_type")?, CORPORA)?;
        let c_compiler = string(get("c_compiler")?, CORPORA)?;
        let cxx_compiler = string(get("cxx_compiler")?, CORPORA)?;
        let configure = array(get("configure")?, CORPORA)?;
        let generate = array(get("generate")?, CORPORA)?;
        for (key, v) in [("build_type", &build_type), ("c_compiler", &c_compiler)]
            .into_iter()
            .chain([("cxx_compiler", &cxx_compiler)])
        {
            if v.is_empty() || v.contains(char::is_whitespace) || v.contains('/') {
                return Err(format!(
                    "{}: `{key}` must be a non-empty name without spaces or `/`",
                    at(get(key)?.line)
                ));
            }
        }
        if let Some(arg) = configure.iter().find(|a| reserved(a)) {
            return Err(format!(
                "{}: `configure` sets `{arg}`, which xtask sets itself",
                at(get("configure")?.line)
            ));
        }
        if generate.is_empty() {
            return Err(format!("{}: `generate` is empty", at(get("generate")?.line)));
        }
        if let Some(t) = generate.iter().find(|t| !valid_target(t)) {
            return Err(format!(
                "{}: `{t}` is not a build target name or `<target>:<file>`",
                at(get("generate")?.line)
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        if let Some(t) = generate.iter().find(|t| !seen.insert(t.as_str())) {
            return Err(format!("{}: `generate` lists `{t}` twice", at(get("generate")?.line)));
        }
        let expected_unparsed = match entries.get("expected_unparsed") {
            None => Vec::new(),
            Some(entry) => parse_expected_unparsed(entry, CORPORA)?,
        };
        let expected_excluded = match entries.get("expected_excluded") {
            None => Vec::new(),
            Some(entry) => parse_expected_excluded(entry, CORPORA)?,
        };
        let Some((_, krow)) = kiln_rows.get(&kiln_name) else {
            return Err(format!(
                "{}: [{name}] names `{kiln_name}`, which is not a table of {KILN_PROJECTS}",
                at(get("kiln")?.line)
            ));
        };
        let (url, tag, source_subdir) = pin(krow, &kiln_name)?;
        out.push(Corpus {
            name,
            kiln: kiln_name,
            url,
            tag,
            source_subdir,
            cxx_std,
            build_type,
            c_compiler,
            cxx_compiler,
            configure,
            generate,
            expected_unparsed,
            expected_excluded,
        });
    }
    Ok(out)
}

/// The URL, tag and source directory of the Kiln row `kiln_name`.
fn pin(
    krow: &BTreeMap<String, Entry>,
    kiln_name: &str,
) -> Result<(String, String, Option<String>), String> {
    let kat = |key: &str| format!("{KILN_PROJECTS} [{kiln_name}] `{key}`");
    for key in ["archive", "local", "submodules", "stage_of"] {
        if let Some(e) = krow.get(key)
            && e.value != "false"
        {
            return Err(format!(
                "{}: a corpus must be a plain git clone; this row sets it",
                kat(key)
            ));
        }
    }
    let kget = |key: &str| {
        krow.get(key).ok_or_else(|| format!("{KILN_PROJECTS}: [{kiln_name}] has no `{key}`"))
    };
    let url = string(kget("url")?, KILN_PROJECTS)?;
    let tag = string(kget("tag")?, KILN_PROJECTS)?;
    if url.is_empty() || url.starts_with('-') || url.contains(char::is_whitespace) {
        return Err(format!("{}: not a usable git URL", kat("url")));
    }
    if !valid_tag(&tag) {
        return Err(format!("{}: `{tag}` is not a usable tag name", kat("tag")));
    }
    let source_subdir = match krow.get("source_subdir") {
        None => None,
        Some(e) => {
            let s = string(e, KILN_PROJECTS)?;
            if s.is_empty()
                || s.starts_with('/')
                || s.split('/').any(|c| c.is_empty() || c == "." || c == "..")
            {
                return Err(format!("{}: `{s}` is not a relative path", kat("source_subdir")));
            }
            Some(s)
        }
    };
    Ok((url, tag, source_subdir))
}

/// The named corpus of `corpora`.
pub fn find<'a>(corpora: &'a [Corpus], name: &str) -> Result<&'a Corpus, String> {
    corpora.iter().find(|c| c.name == name).ok_or_else(|| {
        let names: Vec<&str> = corpora.iter().map(|c| c.name.as_str()).collect();
        format!("no corpus `{name}` in {CORPORA} (corpora: {})", names.join(", "))
    })
}

type Tables = BTreeMap<String, (usize, BTreeMap<String, Entry>)>;

/// The tables of `text` with their header line and entries by key;
/// entries before the first table, nested or array tables, and duplicate
/// tables or keys are errors.
fn tables(text: &str, file: &str) -> Result<Tables, String> {
    let (headers, entries) = manifest::parse(text);
    let mut out: Tables = BTreeMap::new();
    for h in &headers {
        if h.array || h.table.contains('.') || h.table.is_empty() {
            return Err(format!("{file}:{}: unsupported table header `{}`", h.line, h.table));
        }
        if out.insert(h.table.clone(), (h.line, BTreeMap::new())).is_some() {
            return Err(format!("{file}:{}: table [{}] appears twice", h.line, h.table));
        }
    }
    for e in entries {
        let Some((_, keys)) = out.get_mut(&e.table) else {
            return Err(format!("{file}:{}: `{}` is outside any table", e.line, e.key));
        };
        let (line, key) = (e.line, e.key.clone());
        if keys.insert(key.clone(), e).is_some() {
            return Err(format!("{file}:{line}: key `{key}` appears twice"));
        }
    }
    Ok(out)
}

fn string(e: &Entry, file: &str) -> Result<String, String> {
    let v = e.value.trim();
    let quoted = v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
    let s = manifest::strings(v);
    if !quoted || s.len() != 1 {
        return Err(format!("{file}:{}: `{}` must be one string", e.line, e.key));
    }
    Ok(s.into_iter().next().unwrap_or_default())
}

fn integer(e: &Entry, file: &str) -> Result<u32, String> {
    e.value
        .trim()
        .parse()
        .map_err(|_| format!("{file}:{}: `{}` must be a non-negative integer", e.line, e.key))
}

fn array(e: &Entry, file: &str) -> Result<Vec<String>, String> {
    let v = e.value.trim();
    let Some(inner) = v.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return Err(format!("{file}:{}: `{}` must be an array of strings", e.line, e.key));
    };
    let items = manifest::strings(inner);
    // Every character outside the strings must be a separator: anything
    // else is a value this reader does not understand.
    let mut rest = inner.to_owned();
    for s in &items {
        for q in ['"', '\''] {
            let lit = format!("{q}{s}{q}");
            if let Some(i) = rest.find(&lit) {
                rest.replace_range(i..i + lit.len(), "");
                break;
            }
        }
    }
    if rest.chars().any(|c| !(c == ',' || c.is_whitespace())) {
        return Err(format!("{file}:{}: `{}` must be an array of strings", e.line, e.key));
    }
    Ok(items)
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// A `generate` entry: a target name, or `<target>:<file>` for one
/// generated file a target's rules make (a relative path inside the build
/// tree with a generated-file extension).
fn valid_target(s: &str) -> bool {
    match s.split_once(':') {
        Some((t, file)) => {
            target_name(t)
                && !file.is_empty()
                && !file.starts_with('/')
                && !file.contains(char::is_whitespace)
                && !file.contains(':')
                && file.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
                && Path::new(file)
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| super::build_tree::GENERATED.contains(&e))
        }
        None => target_name(s),
    }
}

fn target_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.+".contains(&b))
}

fn valid_tag(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-./+".contains(&b))
}

fn reserved(arg: &str) -> bool {
    // The generator, the source and build directories and `--fresh` are
    // xtask's too.
    if ["-G", "-S", "-B"].iter().any(|p| arg.starts_with(p)) || arg == "--fresh" {
        return true;
    }
    let Some(var) = arg.strip_prefix("-D") else { return false };
    let var = var.split(['=', ':']).next().unwrap_or_default();
    RESERVED.contains(&var)
}

fn valid_src_rel_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(char::is_whitespace)
        && path.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// Parses `expected_unparsed` entries of the form `<outcome>:<path>:<reason>`.
fn parse_expected_unparsed(e: &Entry, file: &str) -> Result<Vec<ExpectedUnparsed>, String> {
    let raw = array(e, file)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(raw.len());
    for item in raw {
        let mut parts = item.splitn(3, ':');
        let (Some(outcome), Some(path), Some(reason)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(format!(
                "{file}:{}: `expected_unparsed` entry `{item}` must be `<outcome>:<path>:<reason>`",
                e.line
            ));
        };
        let status = match outcome {
            "diagnosed" => Status::Diagnosed,
            "rejected" => Status::Rejected,
            "failed" => Status::Failed,
            _ => {
                return Err(format!(
                    "{file}:{}: `expected_unparsed` outcome `{outcome}` must be `diagnosed`, \
                     `rejected` or `failed`",
                    e.line
                ));
            }
        };
        if !valid_src_rel_path(path) {
            return Err(format!(
                "{file}:{}: `expected_unparsed` path `{path}` is not a relative path under `src/`",
                e.line
            ));
        }
        let reason = reason.trim();
        if reason.is_empty() || reason.contains(['\n', '\r']) {
            return Err(format!(
                "{file}:{}: `expected_unparsed` entry `{item}` needs a one-line reason",
                e.line
            ));
        }
        if !seen.insert(path.to_owned()) {
            return Err(format!("{file}:{}: `expected_unparsed` lists `{path}` twice", e.line));
        }
        out.push(ExpectedUnparsed { path: path.to_owned(), status, reason: reason.to_owned() });
    }
    Ok(out)
}

/// Parses `expected_excluded` entries of the form `<reason>:<path>:<note>`.
fn parse_expected_excluded(e: &Entry, file: &str) -> Result<Vec<ExpectedExcluded>, String> {
    let raw = array(e, file)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(raw.len());
    for item in raw {
        let mut parts = item.splitn(3, ':');
        let (Some(reason_str), Some(path), Some(note)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(format!(
                "{file}:{}: `expected_excluded` entry `{item}` must be `<reason>:<path>:<note>`",
                e.line
            ));
        };
        let Some(reason) =
            crate::perf::REASONS.into_iter().find(|r| crate::perf::reason_name(*r) == reason_str)
        else {
            let valid: Vec<&str> =
                crate::perf::REASONS.into_iter().map(crate::perf::reason_name).collect();
            return Err(format!(
                "{file}:{}: `expected_excluded` reason `{reason_str}` must be one of {}",
                e.line,
                valid.join(", ")
            ));
        };
        if !valid_src_rel_path(path) {
            return Err(format!(
                "{file}:{}: `expected_excluded` path `{path}` is not a relative path under `src/`",
                e.line
            ));
        }
        let note = note.trim();
        if note.is_empty() || note.contains(['\n', '\r']) {
            return Err(format!(
                "{file}:{}: `expected_excluded` entry `{item}` needs a one-line note",
                e.line
            ));
        }
        if !seen.insert(path.to_owned()) {
            return Err(format!("{file}:{}: `expected_excluded` lists `{path}` twice", e.line));
        }
        out.push(ExpectedExcluded { path: path.to_owned(), reason, note: note.to_owned() });
    }
    Ok(out)
}
