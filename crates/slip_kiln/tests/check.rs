//! `spec/cdb.md` §6, §6.1 and §6.2: `KilnCheck`, Kiln's
//! command-line parser as the authority on flags and language.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use slip_cdb::check::{ByExtension, FrontEndCheck, Rejection};
use slip_kiln::{KilnCheck, parse_observer};
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard, Status};

fn command(args: &[&str]) -> Command {
    Command {
        directory: b"/work".to_vec(),
        argv: args.iter().map(|a| a.as_bytes().to_vec()).collect(),
        env: Vec::new(),
    }
}

/// `KilnCheck` on `args`, after asserting that the check and the parse
/// agree on it (`agree`).
fn check(args: &[&str]) -> Result<Language, Rejection> {
    let result = KilnCheck.check(&command(args), b"/work/main");
    agree(args, &result);
    result
}

/// Parses `args` in a fresh directory where each relative argument that
/// does not start with `-` is an empty file.
fn parse_in_fixture(args: &[&str]) -> Shard {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "slip-kiln-check-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    for a in &args[1..] {
        let p = Path::new(a);
        if a.starts_with('-') || p.is_absolute() || a.contains("..") || a.is_empty() {
            continue;
        }
        let p = dir.join(p);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let _ = std::fs::write(&p, b"");
    }
    let mut c = command(args);
    c.directory = dir.to_str().unwrap().as_bytes().to_vec();
    let o = Observer {
        id: Digest([1; 32]),
        command: c,
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let shard = parse_observer(&o, Phase::Map, 0);
    let _ = std::fs::remove_dir_all(&dir);
    shard.expect("parse_observer")
}

/// The parse refused the command for a reason of §6.2 rejection 3: what
/// `run_front_end` refuses without running, or with
/// `drv_front_end_unsupported`.
fn refused_by_entry(shard: &Shard) -> bool {
    shard.outcome.status == Status::Rejected
        && (shard.outcome.message == "the command line is not -fsyntax-only"
            || shard.outcome.message == "the command line has not one source input"
            || shard.outcome.message.contains("is not supported by the front-end library entry"))
}

/// §6.2: a command `KilnCheck` accepts is never refused by the parse for a
/// reason of rejection 3.
fn agree(args: &[&str], result: &Result<Language, Rejection>) {
    if result.is_ok() {
        let shard = parse_in_fixture(args);
        assert!(!refused_by_entry(&shard), "KilnCheck accepts {args:?}: {:?}", shard.outcome);
    }
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

// §6.2 rejection 5: other inputs Kiln would not compile as the build's compiler would.
#[test]
fn s6_2_other_languages_refused() {
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
    // WP-163 / WP-148: flags Slip does not model are refused as written,
    // though Kiln accepts them at this pin (`slip_kiln::check::not_modelled`).
    for (argv, flag) in [
        (&["clang++", "-working-directory=sub", "a.cpp"][..], "-working-directory=sub"),
        (&["clang++", "-working-directory", "sub", "a.cpp"][..], "-working-directory"),
        (&["clang++", "-imacros", "m.h", "a.cpp"][..], "-imacros"),
        (&["clang++", "--imacros=m.h", "a.cpp"][..], "--imacros=m.h"),
        (&["clang++", "-iprefix", "p/", "a.cpp"][..], "-iprefix"),
        (&["clang++", "-iwithprefix", "inc", "a.cpp"][..], "-iwithprefix"),
        (&["clang++", "-iwithprefixbefore", "inc", "a.cpp"][..], "-iwithprefixbefore"),
        (&["clang++", "--imacros", "m.h", "a.cpp"][..], "--imacros"),
        (&["clang++", "--include-prefix=p/", "a.cpp"][..], "--include-prefix=p/"),
        (&["clang++", "--include-with-prefix", "inc", "a.cpp"][..], "--include-with-prefix"),
        (
            &["clang++", "--include-with-prefix-after=inc", "a.cpp"][..],
            "--include-with-prefix-after=inc",
        ),
        (
            &["clang++", "--include-with-prefix-before", "inc", "a.cpp"][..],
            "--include-with-prefix-before",
        ),
        // Tables, checks and dumps Kiln added for a tooling client
        // (fix round 1 of WP-163): refused at the old pin, accepted now.
        (&["clang++", "-fkiln-written-ref-keys", "a.cpp"][..], "-fkiln-written-ref-keys"),
        (&["clang++", "-fkiln-fn-names", "a.cpp"][..], "-fkiln-fn-names"),
        (&["clang++", "-fkiln-builtin-literals", "a.cpp"][..], "-fkiln-builtin-literals"),
        (&["clang++", "-fkiln-verify-subst-failed", "a.cpp"][..], "-fkiln-verify-subst-failed"),
        (&["clang++", "-fkiln-dump-friend-forms", "a.cpp"][..], "-fkiln-dump-friend-forms"),
        (&["clang++", "-fkiln-dump-fn-names", "a.cpp"][..], "-fkiln-dump-fn-names"),
    ] {
        assert_eq!(check(argv), Err(Rejection::UnsupportedFlag(flag.into())), "{argv:?}");
    }
    // A separate-value option left without one at the end (rejection 2).
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

// §6.2 rejection 3: what `run_front_end` refuses without running and what
// it refuses with `drv_front_end_unsupported` when written; the argument
// responsible as the detail; and the parse refuses each one too.
#[test]
fn s6_2_rejection_3_front_end_refusals() {
    let rows: &[(&[&str], &str)] = &[
        (&["clang++", "-E", "a.cpp"], "-E"),
        (&["clang++", "--help"], "--help"),
        (&["clang++", "a.cpp", "-###"], "-###"),
        (&["clang++", "-dumpversion", "a.cpp"], "-dumpversion"),
        (&["clang++", "-dumpmachine", "a.cpp"], "-dumpmachine"),
        (&["clang++", "-print-target-triple", "a.cpp"], "-print-target-triple"),
        (&["clang++", "-print-file-name=libc.so", "a.cpp"], "-print-file-name=libc.so"),
        (&["clang++", "-print-resource-dir", "a.cpp"], "-print-resource-dir"),
        (&["clang++", "-M", "a.cpp"], "-M"),
        (&["clang++", "-Iinc", "-MM", "a.cpp"], "-MM"),
        (&["clang++", "-fmodule-header=user", "a.cpp"], "-fmodule-header=user"),
        (&["clang++", "-verify", "a.cpp"], "-verify"),
        (&["clang++", "-include-pch", "pre.pch", "a.cpp"], "-include-pch"),
        (&["clang++", "-x", "c++", "-"], "-"),
    ];
    for (argv, culprit) in rows {
        assert_eq!(
            KilnCheck.check(&command(argv), b"/work/main"),
            Err(Rejection::UnsupportedFlag((*culprit).into())),
            "{argv:?}"
        );
        if argv.contains(&"--help") {
            // No input: the entry says `NotSyntaxOnly` before it counts.
            continue;
        }
        let shard = parse_in_fixture(argv);
        assert!(refused_by_entry(&shard), "{argv:?}: {:?}", shard.outcome);
    }
    // What the parse used to rely on and no longer does is accepted: the
    // entry writes nothing and renders nothing.
    for argv in [
        &["clang++", "-Wp,-MMD,x.d", "a.cpp"][..],
        &["clang++", "-MD", "-MF", "x.d", "-MT", "x.o", "-MP", "a.cpp"],
        &["clang++", "-o", "x.o", "-MJ", "x.json", "a.cpp"],
        &["clang++", "-fdiagnostics-format=sarif", "a.cpp"],
        &["clang++", "-fdiagnostics-format=json", "a.cpp"],
        &["clang++", "-c", "a.cpp"],
        // `-fsyntax-only` comes first and wins over a later phase.
        &["clang++", "-S", "a.cpp"],
        &["clang++", "--precompile", "a.cppm"],
        &["clang++", "-fmodule-header", "a.h"],
    ] {
        assert_eq!(check(argv), Ok(Language::Cxx), "{argv:?}");
    }
}

// §6.1: the line checked is `argv[0] -fsyntax-only
// -fkiln-reproducible-time argv[1..]`. The two come first, where no
// argument of the command takes either as its value: a separate-value
// option at the end of the command lacks its value whatever comes before
// it; the command writing `-fkiln-reproducible-time` itself is accepted.
#[test]
fn s6_1_checked_line() {
    assert_eq!(check(&["clang++", "-o"]), Err(Rejection::UnsupportedFlag("-o".into())));
    assert_eq!(check(&["clang++", "a.cpp", "-x"]), Err(Rejection::UnsupportedFlag("-x".into())));
    assert_eq!(check(&["clang++", "-fkiln-reproducible-time", "a.cpp"]), Ok(Language::Cxx));
    // `-o` takes the input, which leaves none.
    assert!(matches!(check(&["clang++", "-o", "a.cpp"]), Err(Rejection::UnsupportedLanguage(_))));
}

// WP-148 fix round 1 (M10): an unmodelled flag (`-imacros`) inside a relative
// response file is expanded relative to the entry's directory by
// `slip_cdb::normalise` and refused by `KilnCheck`; and if passed directly to
// `parse_observer` without normalisation, `response_files = false` refuses the
// `@` word without reading the response file.
#[test]
fn s6_2_unmodelled_flag_in_relative_rsp_is_refused() {
    let dir = std::env::temp_dir().join(format!("slip-kiln-check-rsp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    std::fs::write(dir.join("m.h"), "#define M 1\n").unwrap();
    std::fs::write(dir.join("flags.rsp"), "-imacros m.h\n").unwrap();
    std::fs::write(dir.join("main.cpp"), "int x = 1;\n").unwrap();

    let raw: Vec<String> =
        ["c++", "-c", "@flags.rsp", "main.cpp"].into_iter().map(str::to_owned).collect();
    let norm = slip_cdb::normalise::normalise(dir.to_str().unwrap(), &raw, "main.cpp")
        .expect("normalise expands relative @flags.rsp");
    assert_eq!(
        KilnCheck.check(&norm.command, norm.main.to_str().unwrap().as_bytes()),
        Err(Rejection::UnsupportedFlag("-imacros".into()))
    );

    let obs = Observer {
        id: Digest([2; 32]),
        command: Command {
            directory: dir.to_str().unwrap().as_bytes().to_vec(),
            argv: ["c++", "@flags.rsp", "main.cpp"]
                .into_iter()
                .map(|a| a.as_bytes().to_vec())
                .collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let shard = parse_observer(&obs, Phase::Map, 0).expect("parse_observer");
    assert_eq!(shard.outcome.status, Status::Rejected, "{:?}", shard.outcome);
    let _ = std::fs::remove_dir_all(&dir);
}
