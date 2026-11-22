//! The Kiln-backed front-end check (`spec/cdb.md` §6, §6.1, §6.2). Work
//! package WP-010.
//!
//! Kiln's own command-line parser (`kiln_driver::args::Args::parse_as`)
//! decides both questions: whether every flag is accepted, and which
//! language the input is compiled as. Nothing is read from disk and nothing
//! is compiled.

use kiln_driver::args::{Action, ArgError, Args, AsmKind, DepHeaders, Input, Lang};
use slip_cdb::check::{FrontEndCheck, Rejection};
use slip_model::{Command, Language};

/// The target the parse task names with `-MT`.
pub(crate) const DEP_TARGET: &str = "slip.d";

/// The dependency file the check stands in for the parse task's (never
/// opened: the check only parses the command line).
const CHECK_DEP_FILE: &str = "/slip-check/slip.d";

/// The command line the parse task gives Kiln (`spec/cdb.md` §6.1):
/// its own options first, where no argument of the command can take them
/// as a value, then the command's arguments, then the diagnostic format,
/// last so that it wins over the command's own.
pub(crate) fn front_end_line(rest: &[String], dep_file: &str) -> Vec<String> {
    let mut line: Vec<String> = ["-fsyntax-only", "-MD", "-MF", dep_file, "-MT", DEP_TARGET]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    line.extend(rest.iter().cloned());
    line.push(DIAGNOSTICS_FORMAT.to_owned());
    line
}

const DIAGNOSTICS_FORMAT: &str = "-fdiagnostics-format=json";

/// Whether the command's own arguments override what the parse task
/// relies on: the action, the JSON diagnostics, and a dependency file of
/// every header at `dep_file` under one `-MT` target. `Some(argument)`
/// names the argument responsible, as well as it can be told.
pub(crate) fn overridden(args: &Args, rest: &[String], dep_file: &str) -> Option<String> {
    if args.diag_args.last().map(String::as_str) != Some(DIAGNOSTICS_FORMAT) {
        // Only an option that takes a separate value, left without one at
        // the end of the command, can have taken it.
        return Some(rest.last().cloned().unwrap_or_default());
    }
    let deps = &args.deps;
    let deps_ok = deps.only.is_none()
        && deps.side == Some(DepHeaders::All)
        && deps.file.as_deref() == Some(dep_file)
        && deps.targets == [(DEP_TARGET.to_owned(), false)]
        && !deps.phony
        && !deps.missing_generated
        && deps.cdb_fragment.is_none();
    if args.action != Action::SyntaxOnly || !deps_ok {
        let culprit = rest
            .iter()
            .find(|a| a.starts_with("-Wp,") || a.starts_with("-M") || a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| "an option that overrides -fsyntax-only or -MD".to_owned());
        return Some(culprit);
    }
    None
}

/// The front-end check of the linked Kiln (`spec/cdb.md` §6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KilnCheck;

impl FrontEndCheck for KilnCheck {
    fn check(&self, command: &Command, _main: &[u8]) -> Result<Language, Rejection> {
        let (argv0, rest) = utf8_command(command).map_err(Rejection::UnsupportedFlag)?;
        let line = front_end_line(&rest, CHECK_DEP_FILE);
        let args = Args::parse_as(&program(&argv0), &line);

        if let Some(error) = args.errors.iter().find(|e| is_error(e)) {
            return Err(match error {
                // `language not recognized`: a `-x` value Kiln does not compile.
                ArgError::InvalidValue(option, value) if option == "-x" => {
                    Rejection::UnsupportedLanguage(value.clone())
                }
                _ => Rejection::UnsupportedFlag(written_argument(error, &rest)),
            });
        }
        if let Some(argument) = overridden(&args, &rest, CHECK_DEP_FILE) {
            return Err(Rejection::UnsupportedFlag(argument));
        }

        let input = match args.inputs.as_slice() {
            [input] => input,
            [] => {
                return Err(Rejection::UnsupportedLanguage(
                    "Kiln finds no input in the command".into(),
                ));
            }
            several => {
                return Err(Rejection::UnsupportedLanguage(format!(
                    "Kiln finds {} inputs in the command",
                    several.len()
                )));
            }
        };
        input_language(&args, input)
    }
}

/// The program name the driver is run as. `argv[0]` itself, except a name
/// that would make `kiln_driver::run_as` start its dependency scanner
/// (`clang-scan-deps…`): that runs as `kiln`, the driver mode such a name
/// has anyway (`spec/cdb.md` §6.1).
pub(crate) fn program(argv0: &str) -> String {
    if kiln_driver::scan_deps::is_scanner_name(kiln_driver::args::program_basename(argv0)) {
        "kiln".to_owned()
    } else {
        argv0.to_owned()
    }
}

/// `argv[0]` and the arguments after it as text. Kiln's command line is
/// `String`s: an argument that is not UTF-8 cannot be given to it, and is
/// the error (its lossy rendering).
pub(crate) fn utf8_command(command: &Command) -> Result<(String, Vec<String>), String> {
    let mut words = Vec::with_capacity(command.argv.len());
    for arg in &command.argv {
        match std::str::from_utf8(arg) {
            Ok(s) => words.push(s.to_owned()),
            Err(_) => return Err(String::from_utf8_lossy(arg).into_owned()),
        }
    }
    if words.is_empty() {
        return Err(String::new());
    }
    let argv0 = words.remove(0);
    Ok((argv0, words))
}

/// Whether Kiln's driver reports `e` as an error. `DeprecatedArg` is the
/// only `ArgError` reported as a warning (`drv_deprecated_arg`).
fn is_error(e: &ArgError) -> bool {
    !matches!(e, ArgError::DeprecatedArg(..))
}

/// The argument of `line` that `error` is about, as written: the first
/// argument equal to the option Kiln names (the option and value joined when
/// the option ends in `=`), else the first one starting with it; the
/// reconstructed spelling when none does.
fn written_argument(error: &ArgError, line: &[String]) -> String {
    let joined = |option: &str, value: &str| {
        if option.ends_with('=') { format!("{option}{value}") } else { option.to_owned() }
    };
    let key = match error {
        ArgError::Unknown(a)
        | ArgError::UnknownSuggest(a, _)
        | ArgError::MissingValue(a)
        | ArgError::InvalidStdlib(a)
        | ArgError::InvalidInt(a, _)
        | ArgError::InvalidAlignment(a, _)
        | ArgError::InvalidFlagValue(a, _)
        | ArgError::UnsupportedForTarget(a, _)
        | ArgError::UnsupportedByKiln(a)
        | ArgError::OnlyAllowedWith(a, _)
        | ArgError::NotAllowedWith(a, _)
        | ArgError::DeprecatedArg(a, _)
        | ArgError::NoSuchFile(a)
        | ArgError::MalformedIgnorelist(a)
        | ArgError::MalformedCoverageList(_, a) => a.clone(),
        ArgError::InvalidValue(option, value)
        | ArgError::UnsupportedValue(option, value)
        | ArgError::UnsupportedValueForTarget(option, value, _) => joined(option, value),
        ArgError::InvalidVerifyPrefix(prefix) => format!("-verify={prefix}"),
        ArgError::InvalidArgumentToOption(value, option) => format!("-{option}{value}"),
        ArgError::PatchableEntryOffset => "-fpatchable-function-entry=".to_owned(),
        ArgError::IncompatibleFpEvalMethod(_) => "-ffp-eval-method=".to_owned(),
        ArgError::MemtagStackRequiresMte => "-fsanitize=memtag-stack".to_owned(),
    };
    if key.is_empty() {
        return key;
    }
    line.iter()
        .find(|a| **a == key)
        .or_else(|| line.iter().find(|a| a.starts_with(&key)))
        .cloned()
        .unwrap_or(key)
}

/// The extensions Kiln types as C or C++ without `-x` (`kiln/spec/driver.md`
/// "Input languages"; case-sensitive). Kiln compiles any other extension
/// as C++ under `-fsyntax-only`, where the build's own compiler would not:
/// those are refused (`spec/cdb.md` §6.2).
const KILN_SOURCE_EXTENSIONS: &[&str] = &[
    "c", "h", "i", "cc", "cp", "cpp", "CPP", "cxx", "c++", "C", "ii", "cppm", "ixx", "hh", "hpp",
    "hxx", "H", "inc",
];

/// The language Kiln compiles `input` as, or why it does not.
fn input_language(args: &Args, input: &Input) -> Result<Language, Rejection> {
    if let Some(asm) = input.asm {
        return Err(Rejection::UnsupportedLanguage(
            match asm {
                AsmKind::Plain => "assembler",
                AsmKind::Cpp => "assembler-with-cpp",
            }
            .to_owned(),
        ));
    }
    if input.lang.is_none() {
        let extension =
            std::path::Path::new(&input.path).extension().and_then(|e| e.to_str()).unwrap_or("");
        if !KILN_SOURCE_EXTENSIONS.contains(&extension) {
            return Err(Rejection::UnsupportedLanguage(match extension {
                "m" => "objective-c".to_owned(),
                "mm" | "M" => "objective-c++".to_owned(),
                "" => format!("input `{}` has no extension", input.path),
                e => format!(".{e}"),
            }));
        }
    }
    Ok(match args.input_lang(input) {
        Lang::C => Language::C,
        Lang::Cxx => Language::Cxx,
    })
}
