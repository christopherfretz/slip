//! Command normalisation (`spec/cdb.md` §5). Work package WP-003.
//!
//! The option tables of §5 are the constants at the top of this file, each
//! citing the row it encodes. They are the whole truth: an option not listed
//! is kept as one argument.

use std::path::{Path, PathBuf};

use slip_model::{Command, Reason};

use crate::rsp;

// ---------------------------------------------------------------------------
// The tables of spec/cdb.md §5.
// ---------------------------------------------------------------------------

/// §5 step 1: launchers, removed while at least one argument follows.
pub const LAUNCHERS: &[&str] = &["ccache", "sccache", "distcc", "icecc", "gomacc"];

/// §5 step 2: compilers (base name, lower-cased, without `.exe`) that are
/// `unsupported_compiler`.
pub const UNSUPPORTED_COMPILERS: &[&str] = &["cl", "clang-cl"];

/// §5 step 3, "Removed", row "alone".
pub const REMOVED_ALONE: &[&str] = &[
    "-c",
    "-S",
    "-emit-llvm",
    "-MD",
    "-MMD",
    "-MP",
    "-MG",
    "-MV",
    "-pipe",
    "-fcolor-diagnostics",
    "-fno-color-diagnostics",
];

/// §5 step 3, "Removed", row "with a separate value, removed together".
pub const REMOVED_SEPARATE: &[&str] =
    &["-o", "-MF", "-MT", "-MQ", "-MJ", "--serialize-diagnostics", "-dependency-file"];

/// §5 step 3, "Removed", row "joined": `<prefix><x>` for any non-empty `<x>`.
pub const REMOVED_JOINED_NON_EMPTY: &[&str] = &["-o", "-MF", "-MT", "-MQ", "-MJ"];

/// §5 step 3, "Removed", row "joined": `<prefix><x>` for any `<x>`.
pub const REMOVED_JOINED_ANY: &[&str] = &["--output=", "-fdiagnostics-color="];

/// §5 step 3, "Removed", row "joined": the spellings with no value.
pub const REMOVED_JOINED_EXACT: &[&str] = &["-fdiagnostics-color"];

/// §5 step 3, "Kept, and known to take a separate value". An option takes
/// the next argument only when the argument is exactly its spelling.
pub const KEPT_SEPARATE: &[&str] = &[
    "-I",
    "-D",
    "-U",
    "-F",
    "-L",
    "-l",
    "-B",
    "-x",
    "-u",
    "-T",
    "-z",
    "-G",
    "-arch",
    "-target",
    "-include",
    "-imacros",
    "-isystem",
    "-isysroot",
    "-iquote",
    "-idirafter",
    "-iprefix",
    "-iwithprefix",
    "-iwithprefixbefore",
    "-iframework",
    "-isystem-after",
    "-cxx-isystem",
    "-internal-isystem",
    "-internal-externc-isystem",
    "--sysroot",
    "--param",
    "--include",
    "--include-directory",
    "--define-macro",
    "--undefine-macro",
    "--language",
    "--target",
    "--gcc-toolchain",
    "-Xclang",
    "-Xpreprocessor",
    "-Xassembler",
    "-Xlinker",
    "-Xanalyzer",
    "-mllvm",
    "-working-directory",
    "-std",
    "-fmodule-file",
    "-fmodule-map-file",
    "-ivfsoverlay",
    "-include-pch",
    "-stdlib++-isystem",
    "-MACHINE",
];

/// §5 step 4: action options, matched exactly.
pub const ACTIONS: &[&str] = &["-E", "-M", "-MM", "--version", "--help", "-###"];

/// §5 step 4: action options, matched by prefix.
pub const ACTION_PREFIXES: &[&str] = &["-print-", "--print-"];

/// §5 step 4: an action only when the command has no input.
pub const VERBOSE: &str = "-v";

/// §5 step 5: input extensions, compared exactly, that are `not_a_compile`.
pub const NOT_SOURCE_EXTENSIONS: &[&str] =
    &["o", "obj", "a", "so", "dylib", "lib", "s", "S", "asm", "ll", "bc", "rc"];

// ---------------------------------------------------------------------------
// Interface.
// ---------------------------------------------------------------------------

/// Why an entry did not become a command (`spec/cdb.md` §4, §5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exclusion {
    pub reason: Reason,
    /// Human-readable.
    pub detail: String,
}

/// A normalised command and its main file (`spec/cdb.md` §5 "Result").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Normalised {
    pub command: Command,
    /// Real path of the main file.
    pub main: PathBuf,
}

/// Expands response files (§4), then normalises the command (§5).
///
/// `directory` is the entry's working directory, `arguments` its command
/// line (already split), `file` its main file as spelled.
pub fn normalise(
    directory: &str,
    arguments: &[String],
    file: &str,
) -> Result<Normalised, Exclusion> {
    let arguments = rsp::expand(Path::new(directory), arguments)?;

    // Step 1: launchers.
    let mut first = 0;
    while first + 1 < arguments.len() && LAUNCHERS.contains(&base_name(&arguments[first])) {
        first += 1;
    }
    let Some(argv0) = arguments.get(first) else {
        return Err(exclude(Reason::MalformedEntry, "empty command line".into()));
    };

    // Step 2: compiler kind.
    let kind = base_name(argv0).to_lowercase();
    let kind = kind.strip_suffix(".exe").unwrap_or(&kind);
    if UNSUPPORTED_COMPILERS.contains(&kind) {
        return Err(exclude(
            Reason::UnsupportedCompiler,
            format!("`{argv0}` is not a GCC/clang-style compiler"),
        ));
    }

    // Step 3: scan.
    let items = scan(&arguments[first + 1..]).map_err(|option| {
        exclude(Reason::MalformedEntry, format!("`{option}` needs a value and none follows"))
    })?;

    // Step 4: the action.
    let inputs: Vec<&str> = items
        .iter()
        .filter_map(|item| match item {
            Item::Input(input) => Some(*input),
            _ => None,
        })
        .collect();
    for item in &items {
        if let Item::Option(option) = item
            && (is_action(option) || (*option == VERBOSE && inputs.is_empty()))
        {
            return Err(exclude(
                Reason::NotACompile,
                format!("`{option}` does not compile a source file"),
            ));
        }
    }

    // Step 5: inputs.
    let input = match inputs.as_slice() {
        [] => {
            return Err(exclude(Reason::NotACompile, "the command has no input".into()));
        }
        [input] => *input,
        _ => {
            return Err(exclude(
                Reason::MalformedEntry,
                format!("several inputs: {}", quoted_list(&inputs)),
            ));
        }
    };
    if let Some(extension) = Path::new(input).extension()
        && extension.to_str().is_some_and(|e| NOT_SOURCE_EXTENSIONS.contains(&e))
    {
        return Err(exclude(Reason::NotACompile, format!("input `{input}` is not a source file")));
    }
    let directory_path = Path::new(directory);
    let main = std::fs::canonicalize(directory_path.join(input)).map_err(|e| {
        let detail = if e.kind() == std::io::ErrorKind::NotFound {
            format!("main file `{input}` does not exist")
        } else {
            format!("main file `{input}`: {e}")
        };
        exclude(Reason::MissingFile, detail)
    })?;
    let matches = std::fs::canonicalize(directory_path.join(file)).is_ok_and(|f| f == main);
    if !matches {
        return Err(exclude(
            Reason::MalformedEntry,
            format!("file does not match the command: `{file}` is not the input `{input}`"),
        ));
    }

    // Result.
    let mut argv = Vec::with_capacity(items.len() + 1);
    argv.push(argv0.as_bytes().to_vec());
    for item in &items {
        match item {
            Item::Removed => {}
            Item::Option(arg) | Item::Input(arg) => argv.push(arg.as_bytes().to_vec()),
            Item::OptionValue(option, value) => {
                argv.push(option.as_bytes().to_vec());
                argv.push(value.as_bytes().to_vec());
            }
        }
    }
    Ok(Normalised {
        command: Command { directory: directory.as_bytes().to_vec(), argv, env: Vec::new() },
        main,
    })
}

// ---------------------------------------------------------------------------
// The scan, shared with `check`.
// ---------------------------------------------------------------------------

/// One unit of the arguments after `argv[0]` (§5 step 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Item<'a> {
    /// Removed, with its value if it had one.
    Removed,
    /// A kept option, as one argument.
    Option(&'a str),
    /// A kept option of [`KEPT_SEPARATE`] and its separate value.
    OptionValue(&'a str, &'a str),
    /// An input: does not begin with `-`, or is `-` alone.
    Input(&'a str),
}

/// Classifies `arguments` (those after `argv[0]`) left to right (§5 step 3).
/// The error is the separate-value option that has no value.
pub(crate) fn scan(arguments: &[String]) -> Result<Vec<Item<'_>>, &str> {
    let mut items = Vec::with_capacity(arguments.len());
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        let argument = argument.as_str();
        let item = if REMOVED_SEPARATE.contains(&argument) {
            arguments.next().ok_or(argument)?;
            Item::Removed
        } else if KEPT_SEPARATE.contains(&argument) {
            Item::OptionValue(argument, arguments.next().ok_or(argument)?)
        } else if is_removed(argument) {
            Item::Removed
        } else if argument == "-" || !argument.starts_with('-') {
            Item::Input(argument)
        } else {
            Item::Option(argument)
        };
        items.push(item);
    }
    Ok(items)
}

/// §5 step 3, "Removed": the rows "alone" and "joined".
fn is_removed(argument: &str) -> bool {
    REMOVED_ALONE.contains(&argument)
        || REMOVED_JOINED_EXACT.contains(&argument)
        || REMOVED_JOINED_NON_EMPTY
            .iter()
            .any(|p| argument.len() > p.len() && argument.starts_with(p))
        || REMOVED_JOINED_ANY.iter().any(|p| argument.starts_with(p))
}

/// §5 step 4, except `-v`, which depends on the inputs.
fn is_action(option: &str) -> bool {
    ACTIONS.contains(&option) || ACTION_PREFIXES.iter().any(|p| option.starts_with(p))
}

/// The base name: everything after the last `/`.
pub(crate) fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn exclude(reason: Reason, detail: String) -> Exclusion {
    Exclusion { reason, detail }
}

fn quoted_list(words: &[&str]) -> String {
    words.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", ")
}
