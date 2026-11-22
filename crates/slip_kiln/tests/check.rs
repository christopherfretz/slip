//! `spec/cdb.md` §6 and `spec/landed/cdb/WP-010.md`: `KilnCheck`, Kiln's
//! command-line parser as the authority on flags and language.

use slip_cdb::check::{ByExtension, FrontEndCheck, Rejection};
use slip_kiln::KilnCheck;
use slip_model::{Command, Language};

fn command(args: &[&str]) -> Command {
    Command {
        directory: b"/work".to_vec(),
        argv: args.iter().map(|a| a.as_bytes().to_vec()).collect(),
        env: Vec::new(),
    }
}

fn check(args: &[&str]) -> Result<Language, Rejection> {
    KilnCheck.check(&command(args), b"/work/main")
}

// §6: C and C++ by extension, under a C driver name.
#[test]
fn s6_language_by_extension() {
    let rows: &[(&str, Language)] = &[
        ("a.c", Language::C),
        ("a.cc", Language::Cxx),
        ("a.cp", Language::Cxx),
        ("a.cpp", Language::Cxx),
        ("a.CPP", Language::Cxx),
        ("a.cxx", Language::Cxx),
        ("a.c++", Language::Cxx),
        ("a.C", Language::Cxx),
        ("a.ii", Language::Cxx),
        ("a.cppm", Language::Cxx),
        ("a.ixx", Language::Cxx),
    ];
    for (input, language) in rows {
        assert_eq!(check(&["cc", "-c", input]), Ok(*language), "{input}");
    }
}

// §6 / landed: Kiln's driver mode by program name decides what `.c` is.
// Kiln's rule (kiln/spec/driver.md "Driver modes"): the basename, without
// `.exe`, then without a trailing version, ending in `++` is C++ mode.
#[test]
fn s6_language_by_driver_name() {
    let c_names = [
        "cc",
        "gcc",
        "clang",
        "/usr/bin/gcc",
        "gcc-12",
        "clang-19",
        "clang-19.1",
        "x86_64-linux-gnu-gcc-13",
        "/opt/llvm/bin/clang",
        "kiln",
    ];
    let cxx_names = [
        "c++",
        "g++",
        "clang++",
        "/usr/bin/g++",
        "g++-12",
        "clang++-19",
        "clang++-19.1",
        "c++-12.2",
        "x86_64-linux-gnu-g++-13",
        "/opt/llvm/bin/clang++",
        "kiln++",
    ];
    for name in c_names {
        assert_eq!(check(&[name, "-c", "a.c"]), Ok(Language::C), "{name}");
        assert_eq!(check(&[name, "-c", "a.cpp"]), Ok(Language::Cxx), "{name}");
    }
    for name in cxx_names {
        assert_eq!(check(&[name, "-c", "a.c"]), Ok(Language::Cxx), "{name}");
        assert_eq!(check(&[name, "-c", "a.cpp"]), Ok(Language::Cxx), "{name}");
    }
    // A scanner name runs as `kiln` (C mode), not as the scanner.
    assert_eq!(check(&["clang-scan-deps", "a.c"]), Ok(Language::C));
}

// §6: `-x`, the last one before the input; `-x none` resets.
#[test]
fn s6_dash_x() {
    assert_eq!(check(&["cc", "-x", "c++", "a.c"]), Ok(Language::Cxx));
    assert_eq!(check(&["c++", "-x", "c", "a.cpp"]), Ok(Language::C));
    assert_eq!(check(&["c++", "-xc", "a.cpp"]), Ok(Language::C));
    assert_eq!(check(&["cc", "-x", "c", "-x", "c++", "a.c"]), Ok(Language::Cxx));
    assert_eq!(check(&["cc", "-x", "c++", "-x", "none", "a.c"]), Ok(Language::C));
    assert_eq!(check(&["cc", "-x", "c++-header", "a.h"]), Ok(Language::Cxx));
    // A `-x` after the input does not apply to it.
    assert_eq!(check(&["cc", "a.c", "-x", "c++"]), Ok(Language::C));
    // A `-x` with no extension needed.
    assert_eq!(check(&["cc", "-x", "c++", "noext"]), Ok(Language::Cxx));
}

// §6: an Objective-C input is refused, by extension or by `-x`.
#[test]
fn s6_objective_c_refused() {
    let lang = |args: &[&str]| match check(args) {
        Err(Rejection::UnsupportedLanguage(d)) => d,
        other => panic!("{args:?}: {other:?}"),
    };
    assert_eq!(lang(&["clang", "-c", "a.m"]), "objective-c");
    assert_eq!(lang(&["clang", "-c", "a.mm"]), "objective-c++");
    assert_eq!(lang(&["clang", "-c", "a.M"]), "objective-c++");
    assert_eq!(lang(&["clang", "-x", "objective-c", "a.c"]), "objective-c");
    assert_eq!(lang(&["clang", "-x", "objective-c++", "a.cpp"]), "objective-c++");
}

// Landed: other inputs Kiln would not compile as the build's compiler would.
#[test]
fn landed_other_languages_refused() {
    let lang = |args: &[&str]| match check(args) {
        Err(Rejection::UnsupportedLanguage(d)) => d,
        other => panic!("{args:?}: {other:?}"),
    };
    assert_eq!(lang(&["cc", "-c", "a.f90"]), ".f90");
    assert_eq!(lang(&["cc", "-c", "noext"]), "input `noext` has no extension");
    assert_eq!(lang(&["cc", "-x", "assembler-with-cpp", "a.c"]), "assembler-with-cpp");
    assert_eq!(lang(&["cc", "-x", "assembler", "a.c"]), "assembler");
    assert_eq!(lang(&["cc", "-x", "pascal", "a.c"]), "pascal");
    assert!(lang(&["cc", "-c"]).contains("no input"));
    assert!(lang(&["cc", "-c", "a.c", "b.c"]).contains("2 inputs"));
}

// §6: an unknown flag is refused, the flag as detail.
#[test]
fn s6_unknown_flag_refused() {
    assert_eq!(
        check(&["clang++", "-c", "-fslip-no-such-flag", "a.cpp"]),
        Err(Rejection::UnsupportedFlag("-fslip-no-such-flag".into()))
    );
    // The first refused flag.
    assert_eq!(
        check(&["clang++", "-fslip-first", "-fslip-second", "a.cpp"]),
        Err(Rejection::UnsupportedFlag("-fslip-first".into()))
    );
    // A value Kiln refuses: the argument as written.
    assert_eq!(
        check(&["clang++", "-stdlib=nope", "a.cpp"]),
        Err(Rejection::UnsupportedFlag("-stdlib=nope".into()))
    );
    // A missing value.
    assert_eq!(check(&["clang++", "a.cpp", "-I"]), Err(Rejection::UnsupportedFlag("-I".into())));
    // An option Kiln does not have (`--language` is cdb's synonym of `-x`).
    assert_eq!(
        check(&["clang++", "--language=c", "a.cpp"]),
        Err(Rejection::UnsupportedFlag("--language=c".into()))
    );
    // Landed: an argument that overrides what the parse task relies on.
    assert_eq!(
        check(&["clang++", "-Wp,-MMD,x.d", "a.cpp"]),
        Err(Rejection::UnsupportedFlag("-Wp,-MMD,x.d".into()))
    );
    assert_eq!(
        check(&["clang++", "a.cpp", "-dumpdir"]),
        Err(Rejection::UnsupportedFlag("-dumpdir".into()))
    );
    // An argument that is not UTF-8 cannot reach Kiln.
    let mut c = command(&["clang++", "a.cpp"]);
    c.argv.insert(1, b"-D\xff".to_vec());
    assert!(matches!(KilnCheck.check(&c, b"/work/a.cpp"), Err(Rejection::UnsupportedFlag(_))));
}

// §6: a real LLVM-shaped command (normalised: no `-o`, `-c`, `-MD`, `-MF`).
#[test]
fn s6_llvm_command_accepted() {
    let argv = [
        "/usr/bin/clang++-19",
        "-DGTEST_HAS_RTTI=0",
        "-D_DEBUG",
        "-D_GLIBCXX_ASSERTIONS",
        "-D_GNU_SOURCE",
        "-D__STDC_CONSTANT_MACROS",
        "-D__STDC_FORMAT_MACROS",
        "-D__STDC_LIMIT_MACROS",
        "-I/src/llvm-project/build/lib/Support",
        "-I/src/llvm-project/llvm/lib/Support",
        "-I/src/llvm-project/build/include",
        "-I/src/llvm-project/llvm/include",
        "-isystem",
        "/usr/include/libxml2",
        "-fPIC",
        "-fno-semantic-interposition",
        "-fvisibility-inlines-hidden",
        "-Werror=date-time",
        "-Werror=unguarded-availability-new",
        "-Wall",
        "-Wextra",
        "-Wno-unused-parameter",
        "-Wwrite-strings",
        "-Wcast-qual",
        "-Wmissing-field-initializers",
        "-pedantic",
        "-Wno-long-long",
        "-Wc++98-compat-extra-semi",
        "-Wimplicit-fallthrough",
        "-Wcovered-switch-default",
        "-Wno-noexcept-type",
        "-Wnon-virtual-dtor",
        "-Wdelete-non-virtual-dtor",
        "-Wsuggest-override",
        "-Wstring-conversion",
        "-Wmisleading-indentation",
        "-Wctad-maybe-unsupported",
        "-fdiagnostics-color",
        "-ffunction-sections",
        "-fdata-sections",
        "-O3",
        "-DNDEBUG",
        "-std=c++17",
        "-fno-exceptions",
        "-funwind-tables",
        "-fno-rtti",
        "/src/llvm-project/llvm/lib/Support/StringRef.cpp",
    ];
    assert_eq!(check(&argv), Ok(Language::Cxx));
}

// §6: `KilnCheck` and `ByExtension` agree on ordinary commands.
#[test]
fn s6_agrees_with_by_extension() {
    let table: &[&[&str]] = &[
        &["cc", "-c", "a.c"],
        &["gcc", "-O2", "-Iinc", "-DX=1", "a.c"],
        &["g++", "-c", "a.c"],
        &["clang++-17", "-std=c++20", "a.cc"],
        &["c++", "-x", "c", "a.cpp"],
        &["clang", "-x", "c++", "a.c"],
        &["clang", "-x", "c-header", "a.c"],
        &["clang++", "-Wall", "-Wextra", "-g", "-fPIC", "a.cpp"],
        &["clang", "-isystem", "/usr/include", "-include", "pre.h", "a.cxx"],
        &["c++", "-std=gnu++17", "-O0", "a.C"],
        &["gcc-12", "-std=c11", "a.c"],
        &["/usr/bin/cc", "-x", "c++-cpp-output", "a.ii"],
        &["clang", "-c", "a.m"],
        &["clang", "-c", "a.mm"],
        &["clang", "-x", "objective-c", "a.c"],
    ];
    for argv in table {
        let kiln = check(argv);
        let ext = ByExtension.check(&command(argv), b"/work/main");
        match (&kiln, &ext) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{argv:?}"),
            (Err(Rejection::UnsupportedLanguage(_)), Ok(Language::ObjC | Language::ObjCxx)) => {}
            _ => panic!("{argv:?}: Kiln {kiln:?}, ByExtension {ext:?}"),
        }
    }
}
