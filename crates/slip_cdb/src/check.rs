//! The front-end check trait and `ByExtension` (`spec/cdb.md` §6). Work package WP-003.

use std::path::Path;

use slip_model::{Command, Language};

use crate::normalise::{Item, base_name, scan};

/// Would the front end accept a command, and as what language
/// (`spec/cdb.md` §6)?
pub trait FrontEndCheck {
    /// `command` is a normalised command (§5); `main` is the real path of its
    /// main file.
    fn check(&self, command: &Command, main: &[u8]) -> Result<Language, Rejection>;
}

/// Why the front end refuses a command (`spec/cdb.md` §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// Detail: the language.
    UnsupportedLanguage(String),
    /// Detail: the first flag refused.
    UnsupportedFlag(String),
}

/// The check used when no front end is linked, and in tests
/// (`spec/cdb.md` §6). It accepts every flag and determines the language
/// from the last `-x`/`--language` value before the input, otherwise from
/// the input's extension as spelled in the command. `main` is not read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByExtension;

/// §6: `-x`/`--language` values and the language each means.
pub const LANGUAGE_NAMES: &[(&str, Language)] = &[
    ("c", Language::C),
    ("c-header", Language::C),
    ("c-cpp-output", Language::C),
    ("c++", Language::Cxx),
    ("c++-header", Language::Cxx),
    ("c++-cpp-output", Language::Cxx),
    ("objective-c", Language::ObjC),
    ("objective-c-header", Language::ObjC),
    ("objective-c-cpp-output", Language::ObjC),
    ("objective-c++", Language::ObjCxx),
    ("objective-c++-header", Language::ObjCxx),
    ("objective-c++-cpp-output", Language::ObjCxx),
];

/// §6: extensions, compared exactly, and the language each means. `.c` is
/// C++ when `argv[0]` is a C++ driver ([`is_cxx_driver`]).
pub const EXTENSIONS: &[(&str, Language)] = &[
    ("c", Language::C),
    ("cc", Language::Cxx),
    ("cp", Language::Cxx),
    ("cpp", Language::Cxx),
    ("cxx", Language::Cxx),
    ("c++", Language::Cxx),
    ("C", Language::Cxx),
    ("CPP", Language::Cxx),
    ("cppm", Language::Cxx),
    ("ixx", Language::Cxx),
    ("ii", Language::Cxx),
    ("m", Language::ObjC),
    ("mm", Language::ObjCxx),
    ("M", Language::ObjCxx),
];

impl FrontEndCheck for ByExtension {
    fn check(&self, command: &Command, _main: &[u8]) -> Result<Language, Rejection> {
        let argv: Vec<String> =
            command.argv.iter().map(|a| String::from_utf8_lossy(a).into_owned()).collect();
        let Some((argv0, rest)) = argv.split_first() else {
            return Err(Rejection::UnsupportedLanguage("empty command line".into()));
        };
        let items = scan(rest).map_err(|option| Rejection::UnsupportedFlag(option.to_owned()))?;

        let mut language_name: Option<&str> = None;
        let mut input = None;
        for item in &items {
            match *item {
                Item::Input(i) => {
                    if input.is_some() {
                        return Err(Rejection::UnsupportedLanguage(
                            "the command has several inputs".into(),
                        ));
                    }
                    input = Some(i);
                }
                _ if input.is_some() => {}
                Item::OptionValue("-x" | "--language", value) => language_name = Some(value),
                Item::Option(option) => {
                    if let Some(value) = option.strip_prefix("--language=") {
                        language_name = Some(value);
                    } else if let Some(value) = option.strip_prefix("-x")
                        && !value.is_empty()
                    {
                        language_name = Some(value);
                    }
                }
                Item::OptionValue(..) | Item::Removed => {}
            }
        }
        let Some(input) = input else {
            return Err(Rejection::UnsupportedLanguage("the command has no input".into()));
        };

        if let Some(name) = language_name {
            return LANGUAGE_NAMES
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, language)| *language)
                .ok_or_else(|| Rejection::UnsupportedLanguage(name.to_owned()));
        }

        let extension = Path::new(input).extension().and_then(|e| e.to_str()).unwrap_or("");
        if extension == "c" && is_cxx_driver(argv0) {
            return Ok(Language::Cxx);
        }
        EXTENSIONS.iter().find(|(e, _)| *e == extension).map(|(_, language)| *language).ok_or_else(
            || {
                Rejection::UnsupportedLanguage(if extension.is_empty() {
                    format!("input `{input}` has no extension")
                } else {
                    format!(".{extension}")
                })
            },
        )
    }
}

/// §6: the base name of `argv0`, after removing a trailing
/// `-<digits and dots>`, ends in `++` (`g++`, `clang++-17`, `c++-12.2`).
pub fn is_cxx_driver(argv0: &str) -> bool {
    let name = base_name(argv0);
    let name = match name.rsplit_once('-') {
        Some((stem, version))
            if !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.') =>
        {
            stem
        }
        _ => name,
    };
    name.ends_with("++")
}
