//! The Kiln-backed front-end check (`spec/cdb.md` §6, §6.1, §6.2). Work
//! packages WP-010, WP-025.
//!
//! Kiln's own command-line parser (`kiln_driver::args::Args::parse_as`)
//! decides both questions: whether every flag is accepted, and which
//! language the input is compiled as. Nothing is read from disk and nothing
//! is compiled.

use kiln_driver::args::{Action, ArgError, Args, AsmKind, Input, Lang, ModuleHeaderJob};
use slip_cdb::check::{FrontEndCheck, Rejection};
use slip_model::{Command, Language};

/// The command line the parse task gives Kiln and `KilnCheck` checks
/// (`spec/cdb.md` §6.1): `-fsyntax-only` and `-fkiln-reproducible-time`
/// (`__DATE__`, `__TIME__` and `__TIMESTAMP__` at the Unix epoch,
/// `spec/model.md` §8.4), then the command's arguments. Both come first,
/// where no argument of the command can take either as its value.
pub(crate) fn front_end_line(rest: &[String]) -> Vec<String> {
    let mut line = Vec::with_capacity(rest.len() + 2);
    line.push("-fsyntax-only".to_owned());
    line.push("-fkiln-reproducible-time".to_owned());
    line.extend(rest.iter().cloned());
    line
}

/// Whether `run_front_end` refuses `args` (`spec/cdb.md` §6.2 rejection
/// 3): what it refuses without running (`NotSyntaxOnly`, and standard
/// input as `NotOneInput`), and what it refuses with
/// `drv_front_end_unsupported` when written on the command line. `whole`
/// is false for a prefix of the command line, whose input may not have
/// come yet: there `-fmodule-header` without one input counts as refused.
fn refused(args: &Args, whole: bool) -> bool {
    let module_header = args.module_header.is_some()
        && match args.inputs.as_slice() {
            [input] => !matches!(args.module_header_job(input), ModuleHeaderJob::Build),
            _ => !whole,
        };
    args.action != Action::SyntaxOnly
        || args.help
        || args.probe.any()
        || args.print_resource_dir
        || args.print_commands
        || args.deps.only.is_some()
        || matches!(args.inputs.as_slice(), [input] if input.path == "-")
        || module_header
        || args.verify.is_some()
        || args.pch.include.is_some()
        || not_modelled(args).is_some()
}

/// The first flag of `args` that reads a file or names a search directory
/// Slip does not model (`Args::imacros`, `Args::prefixed_dirs`,
/// `Args::iprefix`). Kiln's driver refused every one of them as an unknown
/// argument until Kiln WP-2822 (`c52c27ba8`, in the pin since WP-163);
/// Slip keeps refusing them, here (the check, `spec/cdb.md` §6.2 rejection
/// 3) and before the parse (`parse_observer_with`), so that a command is
/// refused as it was (parity): the lookup guards (`spec/verify.md` §5.3)
/// do not know the files and directories they bring in (`Driver::of`
/// reads neither), and a file read by a flag Slip does not model would be
/// a hole in them.
pub(crate) fn not_modelled(args: &Args) -> Option<&'static str> {
    if !args.imacros.is_empty() {
        Some("-imacros")
    } else if !args.prefixed_dirs.is_empty() {
        Some("-iwithprefix")
    } else if !args.iprefix.is_empty() {
        Some("-iprefix")
    } else {
        None
    }
}

/// The argument of `rest` that makes the front end refuse it: the last of
/// the shortest prefix that is refused; the option before it instead when
/// that option took it as its value (the prefix before it lacked that
/// option's value).
fn refusing_argument(program: &str, rest: &[String]) -> String {
    for k in 1..=rest.len() {
        if !refused(&Args::parse_as(program, &front_end_line(&rest[..k])), false) {
            continue;
        }
        if k >= 2 {
            let before = Args::parse_as(program, &front_end_line(&rest[..k - 1]));
            let option = &rest[k - 2];
            if before.errors.iter().any(|e| matches!(e, ArgError::MissingValue(o) if o == option)) {
                return option.clone();
            }
        }
        return rest[k - 1].clone();
    }
    "an option the front-end library entry refuses".to_owned()
}

/// The front-end check of the linked Kiln (`spec/cdb.md` §6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KilnCheck;

impl FrontEndCheck for KilnCheck {
    fn check(&self, command: &Command, _main: &[u8]) -> Result<Language, Rejection> {
        let (argv0, rest) = utf8_command(command).map_err(Rejection::UnsupportedFlag)?;
        let program = program(&argv0);
        let args = Args::parse_as(&program, &front_end_line(&rest));

        if let Some(error) = args.errors.iter().find(|e| is_error(e)) {
            return Err(match error {
                // `language not recognized`: a `-x` value Kiln does not compile.
                ArgError::InvalidValue(option, value) if option == "-x" => {
                    Rejection::UnsupportedLanguage(value.clone())
                }
                _ => Rejection::UnsupportedFlag(written_argument(error, &rest)),
            });
        }
        if refused(&args, true) {
            return Err(Rejection::UnsupportedFlag(refusing_argument(&program, &rest)));
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
/// that would make Kiln's driver start its dependency scanner
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

#[cfg(test)]
mod tests {
    use super::front_end_line;

    // spec/cdb.md §6.1: the line is `argv[0] -fsyntax-only
    // -fkiln-reproducible-time argv[1..]` (after `argv[0]`).
    #[test]
    fn s6_1_front_end_line() {
        let rest: Vec<String> = ["-c", "-o", "a.o", "a.cpp"].map(str::to_owned).to_vec();
        assert_eq!(
            front_end_line(&rest),
            ["-fsyntax-only", "-fkiln-reproducible-time", "-c", "-o", "a.o", "a.cpp"]
        );
        assert_eq!(front_end_line(&[]), ["-fsyntax-only", "-fkiln-reproducible-time"]);
    }
}
