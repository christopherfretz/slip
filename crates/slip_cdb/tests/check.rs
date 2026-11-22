//! `spec/cdb.md` §6: `ByExtension`, one case per language value, per
//! extension row, and per rejection.

use slip_cdb::check::{ByExtension, FrontEndCheck, Rejection, is_cxx_driver};
use slip_model::{Command, Language};

fn command(args: &[&str]) -> Command {
    Command {
        directory: b"/work".to_vec(),
        argv: args.iter().map(|a| a.as_bytes().to_vec()).collect(),
        env: Vec::new(),
    }
}

fn check(args: &[&str]) -> Result<Language, Rejection> {
    ByExtension.check(&command(args), b"/work/main")
}

fn unsupported(args: &[&str]) -> String {
    match check(args) {
        Err(Rejection::UnsupportedLanguage(detail)) => detail,
        other => panic!("{args:?}: expected UnsupportedLanguage, got {other:?}"),
    }
}

// §6: the extension table, compared exactly.
#[test]
fn s6_extensions() {
    let rows: &[(&str, Language)] = &[
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
    for (extension, language) in rows {
        let input = format!("src/x.{extension}");
        assert_eq!(check(&["clang", &input]), Ok(*language), "{input}");
    }
    assert_eq!(slip_cdb::check::EXTENSIONS, rows);
}

// §6: any other extension, or none, is UnsupportedLanguage.
#[test]
fn s6_other_extensions_unsupported() {
    for input in ["x.h", "x.hpp", "x.Cc", "x.CC", "x.cu", "x.f90", "x.s", "x.MM", "x.Cpp", "x", "-"]
    {
        unsupported(&["clang++", input]);
    }
    assert_eq!(unsupported(&["clang", "x.cu"]), ".cu");
}

// §6: `-x`/`--language` values, including the -header and -cpp-output variants.
#[test]
fn s6_language_values() {
    let rows = [
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
    for (value, language) in rows {
        // All four spellings; the extension (`.h`, unsupported by itself) is
        // overridden.
        let joined_x = format!("-x{value}");
        let joined_language = format!("--language={value}");
        for args in [
            vec!["clang", "-x", value, "x.h"],
            vec!["clang", joined_x.as_str(), "x.h"],
            vec!["clang", "--language", value, "x.h"],
            vec!["clang", joined_language.as_str(), "x.h"],
        ] {
            assert_eq!(check(&args), Ok(language), "{args:?}");
        }
    }
}

// §6: any other `-x` value is UnsupportedLanguage, with the value as detail.
#[test]
fn s6_other_language_values_unsupported() {
    for value in ["cuda", "assembler", "none", "C++", "cpp-output", "c++-module"] {
        assert_eq!(unsupported(&["clang", "-x", value, "x.cpp"]), value);
    }
}

// §6: the last `-x` before the input wins; one after the input is ignored.
#[test]
fn s6_last_language_before_input() {
    assert_eq!(check(&["clang", "-x", "c", "-xc++", "x.c"]), Ok(Language::Cxx));
    assert_eq!(check(&["clang", "-xc++", "x.c", "-x", "objective-c"]), Ok(Language::Cxx));
    assert_eq!(check(&["clang", "x.c", "-x", "c++"]), Ok(Language::C));
}

// §6: a `-x` that is the value of another option is not a language.
#[test]
fn s6_language_as_value_ignored() {
    assert_eq!(check(&["clang", "-Xclang", "-xc++", "x.c"]), Ok(Language::C));
    assert_eq!(check(&["clang", "-D", "-xc++", "x.c"]), Ok(Language::C));
}

// §6: `.c` is C++ when argv[0] is a C++ driver.
#[test]
fn s6_c_with_cxx_driver() {
    for driver in [
        "c++",
        "g++",
        "clang++",
        "/usr/bin/clang++",
        "clang++-17",
        "g++-12.2.0",
        "x86_64-linux-gnu-g++-12",
    ] {
        assert_eq!(check(&[driver, "x.c"]), Ok(Language::Cxx), "{driver}");
    }
    for driver in
        ["cc", "gcc", "clang", "clang-17", "/opt/c++/bin/clang", "g++-12a", "clang++.exe", "c++-"]
    {
        assert_eq!(check(&[driver, "x.c"]), Ok(Language::C), "{driver}");
    }
    // Only `.c` is affected.
    assert_eq!(check(&["g++", "x.m"]), Ok(Language::ObjC));
    // An explicit `-x c` is not overridden by the driver.
    assert_eq!(check(&["g++", "-x", "c", "x.c"]), Ok(Language::C));
}

#[test]
fn s6_is_cxx_driver() {
    assert!(is_cxx_driver("clang++-17.0.1"));
    assert!(!is_cxx_driver("clang-17.0.1"));
    assert!(!is_cxx_driver("++/clang"));
}

// §6: ByExtension accepts every flag.
#[test]
fn s6_accepts_every_flag() {
    assert_eq!(
        check(&[
            "clang++",
            "-fsome-future-flag",
            "-Wall",
            "--frobnicate=3",
            "-mllvm",
            "-x86-asm-syntax=intel",
            "x.cpp"
        ]),
        Ok(Language::Cxx)
    );
}

// §6: a command with no input, or several, has no language.
#[test]
fn s6_input_count() {
    unsupported(&["clang++", "-Wall"]);
    unsupported(&["clang++", "a.cpp", "b.cpp"]);
    unsupported(&[]);
}
