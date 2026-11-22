//! `spec/cdb.md` §5 (normalisation): each step, each table row, each
//! exclusion, idempotence, and commands in the shape `CMake` writes for LLVM.
//! Also §4 (response-file expansion), which `normalise` runs first.

mod common;

use common::{TempDir, strings};
use slip_cdb::normalise::{
    ACTIONS, KEPT_SEPARATE, LAUNCHERS, NOT_SOURCE_EXTENSIONS, Normalised, REMOVED_ALONE,
    REMOVED_SEPARATE, normalise,
};
use slip_cdb::rsp::{MAX_DEPTH, expand};
use slip_cdb::split::split_command;
use slip_model::Reason;

/// A directory holding `main.cpp`, the usual main file.
fn fixture() -> TempDir {
    let t = TempDir::new("normalise");
    t.write("main.cpp", "int main() {}\n");
    t
}

fn ok(t: &TempDir, args: &[&str], file: &str) -> Normalised {
    normalise(t.dir(), &strings(args), file)
        .unwrap_or_else(|e| panic!("normalise({args:?}) excluded: {e:?}"))
}

fn argv(n: &Normalised) -> Vec<String> {
    n.command.argv.iter().map(|a| String::from_utf8(a.clone()).expect("UTF-8")).collect()
}

/// Normalises and returns the argv.
fn norm(t: &TempDir, args: &[&str]) -> Vec<String> {
    argv(&ok(t, args, "main.cpp"))
}

fn reason(t: &TempDir, args: &[&str], file: &str) -> Reason {
    match normalise(t.dir(), &strings(args), file) {
        Ok(n) => panic!("normalise({args:?}) was not excluded: {:?}", argv(&n)),
        Err(e) => e.reason,
    }
}

fn excluded(t: &TempDir, args: &[&str]) -> Reason {
    reason(t, args, "main.cpp")
}

/// Normalising the result again returns it unchanged (§5 "Result").
fn assert_idempotent(directory: &str, n: &Normalised, file: &str) {
    let again = normalise(directory, &argv(n), file).expect("re-normalise");
    assert_eq!(again, *n, "normalisation is not idempotent");
}

// ---------------------------------------------------------------------------
// Step 1: launchers.
// ---------------------------------------------------------------------------

#[test]
fn s5_step1_each_launcher_removed() {
    let t = fixture();
    for launcher in LAUNCHERS {
        assert_eq!(
            norm(&t, &[launcher, "clang++", "main.cpp"]),
            ["clang++", "main.cpp"],
            "{launcher}"
        );
        let path = format!("/usr/local/bin/{launcher}");
        assert_eq!(
            norm(&t, &[&path, "/usr/bin/g++", "main.cpp"]),
            ["/usr/bin/g++", "main.cpp"],
            "{path}"
        );
    }
}

#[test]
fn s5_step1_launchers_chain() {
    let t = fixture();
    assert_eq!(
        norm(&t, &["ccache", "distcc", "clang++", "-c", "main.cpp"]),
        ["clang++", "main.cpp"]
    );
}

#[test]
fn s5_step1_launcher_kept_when_nothing_follows() {
    // `ccache` alone is argv[0]; there is no input.
    let t = fixture();
    assert_eq!(excluded(&t, &["ccache"]), Reason::NotACompile);
    // `ccache main.cpp`: `main.cpp` becomes argv[0], leaving no input.
    assert_eq!(excluded(&t, &["ccache", "main.cpp"]), Reason::NotACompile);
}

#[test]
fn s5_step1_launcher_name_exact() {
    // Base names are compared exactly; `ccache-wrapper` is the compiler.
    let t = fixture();
    assert_eq!(norm(&t, &["ccache-wrapper", "main.cpp"]), ["ccache-wrapper", "main.cpp"]);
}

#[test]
fn s5_step1_launcher_only_at_front() {
    // A launcher name after argv[0] is an input, not a launcher.
    let t = fixture();
    assert_eq!(excluded(&t, &["clang++", "ccache", "main.cpp"]), Reason::MalformedEntry);
}

#[test]
fn s5_argv0_kept_as_spelled() {
    let t = fixture();
    assert_eq!(norm(&t, &["../bin/clang++", "main.cpp"])[0], "../bin/clang++");
}

#[test]
fn s5_empty_arguments_malformed() {
    let t = fixture();
    assert_eq!(excluded(&t, &[]), Reason::MalformedEntry);
}

// ---------------------------------------------------------------------------
// Step 2: compiler kind.
// ---------------------------------------------------------------------------

#[test]
fn s5_step2_cl_unsupported() {
    let t = fixture();
    for compiler in [
        "cl",
        "cl.exe",
        "CL.EXE",
        "Cl.Exe",
        "clang-cl",
        "clang-cl.exe",
        "CLANG-CL",
        "/opt/llvm/bin/clang-cl",
    ] {
        assert_eq!(
            excluded(&t, &[compiler, "/c", "main.cpp"]),
            Reason::UnsupportedCompiler,
            "{compiler}"
        );
    }
    // After a launcher.
    assert_eq!(excluded(&t, &["sccache", "cl.exe", "main.cpp"]), Reason::UnsupportedCompiler);
}

#[test]
fn s5_step2_other_compilers_supported() {
    let t = fixture();
    for compiler in ["clang", "clang++", "gcc", "c++", "clang-cl-wrapper", "cl++", "xcl"] {
        assert_eq!(norm(&t, &[compiler, "main.cpp"]), [compiler, "main.cpp"]);
    }
}

// ---------------------------------------------------------------------------
// Step 3: the "Removed" table.
// ---------------------------------------------------------------------------

#[test]
fn s5_step3_removed_alone_each() {
    let t = fixture();
    for option in REMOVED_ALONE {
        assert_eq!(
            norm(&t, &["clang++", "-Wall", option, "main.cpp"]),
            ["clang++", "-Wall", "main.cpp"],
            "{option}"
        );
    }
}

#[test]
fn s5_step3_removed_alone_list_is_the_spec_row() {
    assert_eq!(
        REMOVED_ALONE,
        [
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
            "-fno-color-diagnostics"
        ]
    );
}

#[test]
fn s5_step3_removed_separate_each() {
    let t = fixture();
    for option in REMOVED_SEPARATE {
        assert_eq!(
            norm(&t, &["clang++", option, "out/x.o", "-Wall", "main.cpp"]),
            ["clang++", "-Wall", "main.cpp"],
            "{option}"
        );
        // The value is taken whatever it looks like: not an action, not an option.
        assert_eq!(
            norm(&t, &["clang++", option, "-E", "main.cpp"]),
            ["clang++", "main.cpp"],
            "{option} -E"
        );
        // Its value is not an input.
        assert_eq!(
            norm(&t, &["clang++", option, "main.cpp", "main.cpp"]),
            ["clang++", "main.cpp"],
            "{option} main.cpp"
        );
        // No value: malformed.
        assert_eq!(
            excluded(&t, &["clang++", "main.cpp", option]),
            Reason::MalformedEntry,
            "{option} without value"
        );
    }
    assert_eq!(
        REMOVED_SEPARATE,
        ["-o", "-MF", "-MT", "-MQ", "-MJ", "--serialize-diagnostics", "-dependency-file"]
    );
}

#[test]
fn s5_step3_removed_joined() {
    let t = fixture();
    for option in [
        "-omain.o",
        "-o/abs/out.o",
        "-MFmain.d",
        "-MTmain.o",
        "-MQmain.o",
        "-MJmain.json",
        "--output=main.o",
        "-fdiagnostics-color=always",
        "-fdiagnostics-color=never",
        "-fdiagnostics-color",
    ] {
        assert_eq!(
            norm(&t, &["clang++", option, "-Wall", "main.cpp"]),
            ["clang++", "-Wall", "main.cpp"],
            "{option}"
        );
    }
}

#[test]
fn s5_step3_removed_joined_takes_nothing() {
    // A joined removed option is one argument; what follows is classified anew.
    let t = fixture();
    assert_eq!(norm(&t, &["clang++", "-omain.o", "main.cpp"]), ["clang++", "main.cpp"]);
}

// ---------------------------------------------------------------------------
// Step 3: the "Kept, separate value" table.
// ---------------------------------------------------------------------------

#[test]
fn s5_step3_kept_separate_each() {
    let t = fixture();
    for option in KEPT_SEPARATE {
        // The value is kept, after its option, even if it looks like an input
        // or like an option (including a removed one or an action).
        for value in ["value.h", "-o", "-E", "-c"] {
            assert_eq!(
                norm(&t, &["clang++", option, value, "main.cpp"]),
                ["clang++", option, value, "main.cpp"],
                "{option} {value}"
            );
        }
        assert_eq!(
            excluded(&t, &["clang++", "main.cpp", option]),
            Reason::MalformedEntry,
            "{option} without value"
        );
    }
}

#[test]
fn s5_step3_kept_separate_list_is_the_spec() {
    let spec = "-I -D -U -F -L -l -B -x -u -T -z -G -arch -target \
        -include -imacros -isystem -isysroot -iquote -idirafter -iprefix \
        -iwithprefix -iwithprefixbefore -iframework -isystem-after \
        -cxx-isystem -internal-isystem -internal-externc-isystem \
        --sysroot --param --include --include-directory --define-macro \
        --undefine-macro --language --target --gcc-toolchain \
        -Xclang -Xpreprocessor -Xassembler -Xlinker -Xanalyzer -mllvm \
        -working-directory -std -fmodule-file -fmodule-map-file \
        -ivfsoverlay -include-pch -stdlib++-isystem -MACHINE";
    let spec: Vec<&str> = spec.split_whitespace().collect();
    assert_eq!(KEPT_SEPARATE, spec.as_slice());
}

#[test]
fn s5_step3_kept_joined_takes_nothing() {
    let t = fixture();
    for option in [
        "-Iinc",
        "-DX=1",
        "-UX",
        "--sysroot=/x",
        "-std=c++17",
        "-xc++",
        "-isystem/usr/include",
        "--include-directory=inc",
        "-includefoo.h",
    ] {
        assert_eq!(
            norm(&t, &["clang++", option, "main.cpp"]),
            ["clang++", option, "main.cpp"],
            "{option}"
        );
    }
}

#[test]
fn s5_step3_other_options_kept_as_one_argument() {
    let t = fixture();
    let args = [
        "clang++",
        "-Wall",
        "-O2",
        "-fno-exceptions",
        "-fsome-future-flag",
        "--unknown",
        "-W",
        "main.cpp",
    ];
    assert_eq!(norm(&t, &args), args);
    // An unknown option is never taken to have a value: what follows it is
    // classified on its own (here, a second input).
    assert_eq!(
        excluded(&t, &["clang++", "-fsome-flag", "value", "main.cpp"]),
        Reason::MalformedEntry
    );
}

#[test]
fn s5_step3_order_kept() {
    let t = fixture();
    assert_eq!(
        norm(&t, &["clang++", "-DB", "-c", "-DA", "main.cpp", "-o", "x.o", "-Ib", "-Ia", "-DB"]),
        ["clang++", "-DB", "-DA", "main.cpp", "-Ib", "-Ia", "-DB"]
    );
}

// ---------------------------------------------------------------------------
// Step 4: the action.
// ---------------------------------------------------------------------------

#[test]
fn s5_step4_each_action_not_a_compile() {
    let t = fixture();
    assert_eq!(ACTIONS, ["-E", "-M", "-MM", "--version", "--help", "-###"]);
    for action in ACTIONS {
        assert_eq!(excluded(&t, &["clang++", action, "main.cpp"]), Reason::NotACompile, "{action}");
        assert_eq!(
            excluded(&t, &["clang++", action]),
            Reason::NotACompile,
            "{action} without input"
        );
    }
}

#[test]
fn s5_step4_print_prefixes_not_a_compile() {
    let t = fixture();
    for action in [
        "-print-file-name=libc.so",
        "-print-resource-dir",
        "-print-",
        "--print-prog-name=ld",
        "--print-search-dirs",
    ] {
        assert_eq!(excluded(&t, &["clang++", action, "main.cpp"]), Reason::NotACompile, "{action}");
    }
}

#[test]
fn s5_step4_verbose() {
    let t = fixture();
    // With no input, `-v` is an action.
    assert_eq!(excluded(&t, &["clang++", "-v"]), Reason::NotACompile);
    // With an input, it is kept.
    assert_eq!(norm(&t, &["clang++", "-v", "-c", "main.cpp"]), ["clang++", "-v", "main.cpp"]);
}

#[test]
fn s5_step4_action_as_value_is_not_an_action() {
    let t = fixture();
    assert_eq!(
        norm(&t, &["clang++", "-Xclang", "-E", "main.cpp"]),
        ["clang++", "-Xclang", "-E", "main.cpp"]
    );
    assert_eq!(norm(&t, &["clang++", "-MT", "--version", "main.cpp"]), ["clang++", "main.cpp"]);
}

#[test]
fn s5_step4_near_actions_are_kept() {
    let t = fixture();
    for option in ["-Ex", "-MMx", "-version", "-printx", "--helpx"] {
        assert_eq!(
            norm(&t, &["clang++", option, "main.cpp"]),
            ["clang++", option, "main.cpp"],
            "{option}"
        );
    }
}

// ---------------------------------------------------------------------------
// Step 5: inputs.
// ---------------------------------------------------------------------------

#[test]
fn s5_step5_no_input_not_a_compile() {
    let t = fixture();
    assert_eq!(excluded(&t, &["clang++", "-c", "-Wall"]), Reason::NotACompile);
    assert_eq!(excluded(&t, &["clang++", "-I", "main.cpp"]), Reason::NotACompile);
}

#[test]
fn s5_step5_several_inputs_malformed() {
    let t = fixture();
    t.write("other.cpp", "");
    let e = normalise(t.dir(), &strings(&["clang++", "-c", "main.cpp", "other.cpp"]), "main.cpp")
        .unwrap_err();
    assert_eq!(e.reason, Reason::MalformedEntry);
    assert!(e.detail.contains("several inputs"), "{}", e.detail);
}

#[test]
fn s5_step5_dash_is_an_input() {
    let t = fixture();
    // `-` is an input; with `main.cpp` there are two.
    assert_eq!(excluded(&t, &["clang++", "-", "main.cpp"]), Reason::MalformedEntry);
    // Alone it is the one input; it names no file in the directory.
    assert_eq!(excluded(&t, &["clang++", "-"]), Reason::MissingFile);
}

#[test]
fn s5_step5_missing_file() {
    let t = fixture();
    let e = normalise(t.dir(), &strings(&["clang++", "gone.cpp"]), "gone.cpp").unwrap_err();
    assert_eq!(e.reason, Reason::MissingFile);
    assert!(e.detail.contains("gone.cpp"), "{}", e.detail);
}

#[test]
fn s5_step5_file_does_not_match() {
    let t = fixture();
    t.write("other.cpp", "");
    let e = normalise(t.dir(), &strings(&["clang++", "main.cpp"]), "other.cpp").unwrap_err();
    assert_eq!(e.reason, Reason::MalformedEntry);
    assert!(e.detail.contains("file does not match the command"), "{}", e.detail);
    // `file` that does not exist at all does not match either.
    assert_eq!(reason(&t, &["clang++", "main.cpp"], "gone.cpp"), Reason::MalformedEntry);
}

#[test]
fn s5_step5_match_by_real_path() {
    let t = fixture();
    t.write("sub/x.cpp", "");
    let real = std::fs::canonicalize(t.path().join("sub/x.cpp")).expect("canonicalize");
    let abs = t.abs("sub/x.cpp");
    for (input, file) in [
        ("sub/x.cpp", abs.as_str()),
        (abs.as_str(), "sub/x.cpp"),
        ("./sub/x.cpp", "sub/../sub/x.cpp"),
        ("sub//x.cpp", "sub/x.cpp"),
    ] {
        let n = ok(&t, &["clang++", input], file);
        assert_eq!(n.main, real, "{input} / {file}");
        // The input is kept as spelled.
        assert_eq!(argv(&n), ["clang++", input]);
    }
}

#[cfg(unix)]
#[test]
fn s5_step5_match_through_symlink() {
    let t = fixture();
    std::os::unix::fs::symlink(t.path().join("main.cpp"), t.path().join("link.cpp"))
        .expect("symlink");
    let n = ok(&t, &["clang++", "link.cpp"], "main.cpp");
    assert_eq!(n.main, std::fs::canonicalize(t.path().join("main.cpp")).expect("canonicalize"));
    assert_eq!(argv(&n), ["clang++", "link.cpp"]);
}

#[test]
fn s5_step5_non_source_extensions_not_a_compile() {
    let t = fixture();
    assert_eq!(
        NOT_SOURCE_EXTENSIONS,
        ["o", "obj", "a", "so", "dylib", "lib", "s", "S", "asm", "ll", "bc", "rc"]
    );
    for extension in NOT_SOURCE_EXTENSIONS {
        let name = format!("input.{extension}");
        t.write(&name, "");
        assert_eq!(reason(&t, &["clang++", "-c", &name], &name), Reason::NotACompile, "{name}");
    }
}

// §5 step 5: the extension is judged before the file is looked for, so a link
// step whose object file is not built yet is `not_a_compile`, not
// `missing_file`.
#[test]
fn s5_step5_non_source_extension_before_existence() {
    let t = fixture();
    assert_eq!(reason(&t, &["clang++", "gone.o", "-o", "app"], "gone.o"), Reason::NotACompile);
    assert_eq!(reason(&t, &["clang++", "gone.o"], "main.cpp"), Reason::NotACompile);
}

#[test]
fn s5_step5_extension_compared_exactly() {
    let t = fixture();
    for name in ["input.O", "input.SO", "input.Asm", "input.ll.cpp", "input.o.c"] {
        t.write(name, "");
        assert_eq!(argv(&ok(&t, &["clang", name], name)), ["clang", name]);
    }
}

// ---------------------------------------------------------------------------
// Result.
// ---------------------------------------------------------------------------

#[test]
fn s5_result_fields() {
    let t = fixture();
    let n = ok(&t, &["clang++", "-c", "main.cpp", "-o", "main.o"], "main.cpp");
    assert_eq!(n.command.directory, t.dir().as_bytes());
    assert_eq!(argv(&n), ["clang++", "main.cpp"]);
    assert!(n.command.env.is_empty());
    assert_eq!(n.main, std::fs::canonicalize(t.path().join("main.cpp")).expect("canonicalize"));
}

#[test]
fn s5_result_directory_as_given() {
    // The directory is kept as given, not canonicalised.
    let t = fixture();
    let spelled = format!("{}/./", t.dir());
    let n = normalise(&spelled, &strings(&["clang++", "main.cpp"]), "main.cpp").expect("ok");
    assert_eq!(n.command.directory, spelled.as_bytes());
}

#[test]
fn s5_idempotent_on_mixed_command() {
    let t = fixture();
    let n = ok(
        &t,
        &[
            "ccache",
            "clang++",
            "-Xclang",
            "-o",
            "-MD",
            "-MF",
            "-Ifoo",
            "-I",
            "-c",
            "-fdiagnostics-color",
            "-o",
            "x.o",
            "-v",
            "main.cpp",
        ],
        "main.cpp",
    );
    assert_eq!(argv(&n), ["clang++", "-Xclang", "-o", "-I", "-c", "-v", "main.cpp"]);
    assert_idempotent(t.dir(), &n, "main.cpp");
}

// ---------------------------------------------------------------------------
// Commands in the shape CMake writes for LLVM.
// ---------------------------------------------------------------------------

/// An LLVM-shaped tree: `src/llvm/...` sources and a `build/` directory.
fn llvm_tree() -> TempDir {
    let t = TempDir::new("llvm");
    t.write("src/llvm/lib/Support/APFloat.cpp", "");
    t.write("src/llvm/lib/Support/regcomp.c", "");
    t.write("src/clang/lib/Sema/SemaDecl.cpp", "");
    t.write("src/libcxx/src/string.cpp", "");
    t.write("src/llvm/utils/TableGen/TableGen.cpp", "");
    std::fs::create_dir_all(t.path().join("build")).expect("mkdir build");
    t
}

/// Runs one CMake-shaped case: `$S` is the source root, `$B` the build root.
fn llvm_case(t: &TempDir, command: &str, file: &str, expected: &str) {
    let s = t.abs("src");
    let b = t.abs("build");
    let subst = |x: &str| x.replace("$S", &s).replace("$B", &b);
    let arguments = split_command(&subst(command)).expect("split");
    let file = subst(file);
    let n = normalise(&b, &arguments, &file).unwrap_or_else(|e| panic!("{e:?}"));
    let expected: Vec<String> = split_command(&subst(expected)).expect("split expected");
    assert_eq!(argv(&n), expected);
    assert_eq!(
        n.main,
        std::fs::canonicalize(std::path::Path::new(&b).join(&file)).expect("canonicalize")
    );
    assert_idempotent(&b, &n, &file);
    // The same command given as `arguments` behaves identically.
    assert_eq!(normalise(&b, &arguments, &file).expect("again"), n, "deterministic");
}

#[test]
fn s5_llvm_support_cxx() {
    let t = llvm_tree();
    llvm_case(
        &t,
        "/usr/bin/c++ -DGTEST_HAS_RTTI=0 -D_DEBUG -D_GLIBCXX_ASSERTIONS -D_GNU_SOURCE \
         -D__STDC_CONSTANT_MACROS -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS \
         -I$B/lib/Support -I$S/llvm/lib/Support -I$B/include -I$S/llvm/include \
         -fPIC -fno-semantic-interposition -fvisibility-inlines-hidden -Werror=date-time \
         -Wall -Wextra -Wno-unused-parameter -Wwrite-strings -Wcast-qual \
         -Wno-missing-field-initializers -pedantic -Wno-long-long -Wimplicit-fallthrough \
         -Wno-maybe-uninitialized -Wno-class-memaccess -Wno-redundant-move \
         -Wno-pessimizing-move -Wno-noexcept-type -Wdelete-non-virtual-dtor \
         -Wsuggest-override -Wno-comment -Wno-misleading-indentation -fdiagnostics-color \
         -ffunction-sections -fdata-sections -O3 -DNDEBUG -std=c++17 -fno-exceptions \
         -funwind-tables -fno-rtti -MD -MT lib/Support/CMakeFiles/LLVMSupport.dir/APFloat.cpp.o \
         -MF lib/Support/CMakeFiles/LLVMSupport.dir/APFloat.cpp.o.d \
         -o lib/Support/CMakeFiles/LLVMSupport.dir/APFloat.cpp.o \
         -c $S/llvm/lib/Support/APFloat.cpp",
        "$S/llvm/lib/Support/APFloat.cpp",
        "/usr/bin/c++ -DGTEST_HAS_RTTI=0 -D_DEBUG -D_GLIBCXX_ASSERTIONS -D_GNU_SOURCE \
         -D__STDC_CONSTANT_MACROS -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS \
         -I$B/lib/Support -I$S/llvm/lib/Support -I$B/include -I$S/llvm/include \
         -fPIC -fno-semantic-interposition -fvisibility-inlines-hidden -Werror=date-time \
         -Wall -Wextra -Wno-unused-parameter -Wwrite-strings -Wcast-qual \
         -Wno-missing-field-initializers -pedantic -Wno-long-long -Wimplicit-fallthrough \
         -Wno-maybe-uninitialized -Wno-class-memaccess -Wno-redundant-move \
         -Wno-pessimizing-move -Wno-noexcept-type -Wdelete-non-virtual-dtor \
         -Wsuggest-override -Wno-comment -Wno-misleading-indentation \
         -ffunction-sections -fdata-sections -O3 -DNDEBUG -std=c++17 -fno-exceptions \
         -funwind-tables -fno-rtti \
         $S/llvm/lib/Support/APFloat.cpp",
    );
}

#[test]
fn s5_llvm_support_c_with_ccache() {
    let t = llvm_tree();
    llvm_case(
        &t,
        "/usr/bin/ccache /usr/bin/cc -D_DEBUG -D_GNU_SOURCE -D__STDC_CONSTANT_MACROS \
         -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS -I$B/lib/Support -I$S/llvm/lib/Support \
         -I$B/include -I$S/llvm/include -fPIC -Werror=date-time -Wall -Wextra \
         -Wno-unused-parameter -Wwrite-strings -Wno-missing-field-initializers -pedantic \
         -Wno-long-long -Wimplicit-fallthrough -Wno-comment -fdiagnostics-color \
         -ffunction-sections -fdata-sections -O3 -DNDEBUG -std=gnu11 \
         -MD -MT lib/Support/CMakeFiles/LLVMSupport.dir/regcomp.c.o \
         -MF lib/Support/CMakeFiles/LLVMSupport.dir/regcomp.c.o.d \
         -o lib/Support/CMakeFiles/LLVMSupport.dir/regcomp.c.o \
         -c $S/llvm/lib/Support/regcomp.c",
        "$S/llvm/lib/Support/regcomp.c",
        "/usr/bin/cc -D_DEBUG -D_GNU_SOURCE -D__STDC_CONSTANT_MACROS \
         -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS -I$B/lib/Support -I$S/llvm/lib/Support \
         -I$B/include -I$S/llvm/include -fPIC -Werror=date-time -Wall -Wextra \
         -Wno-unused-parameter -Wwrite-strings -Wno-missing-field-initializers -pedantic \
         -Wno-long-long -Wimplicit-fallthrough -Wno-comment \
         -ffunction-sections -fdata-sections -O3 -DNDEBUG -std=gnu11 \
         $S/llvm/lib/Support/regcomp.c",
    );
}

#[test]
fn s5_llvm_clang_sema_with_isystem_and_quoted_macro() {
    // clang++ as the host compiler: `-fcolor-diagnostics`, separate
    // `-isystem`, a string-valued macro escaped the way CMake writes it.
    let t = llvm_tree();
    llvm_case(
        &t,
        "/opt/llvm/bin/clang++ -DCLANG_VENDOR=\\\"Mason\\ \\\" -D_DEBUG -D_GNU_SOURCE \
         -D__STDC_CONSTANT_MACROS -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS \
         -I$B/tools/clang/lib/Sema -I$S/clang/lib/Sema -I$S/clang/include \
         -I$B/tools/clang/include -I$B/include -I$S/llvm/include \
         -isystem /usr/include/libxml2 -fPIC -fvisibility-inlines-hidden \
         -Werror=date-time -Werror=unguarded-availability-new -Wall -Wextra \
         -Wno-unused-parameter -Wwrite-strings -Wcast-qual -Wmissing-field-initializers \
         -pedantic -Wno-long-long -Wc++98-compat-extra-semi -Wimplicit-fallthrough \
         -Wcovered-switch-default -Wno-noexcept-type -Wnon-virtual-dtor \
         -Wdelete-non-virtual-dtor -Wsuggest-override -Wstring-conversion \
         -Wmisleading-indentation -Wctad-maybe-unsupported -fdiagnostics-color \
         -ffunction-sections -fdata-sections -fno-common -Woverloaded-virtual \
         -Wno-nested-anon-types -O3 -DNDEBUG -std=c++17 -fno-exceptions -funwind-tables \
         -fno-rtti -UNDEBUG -MD \
         -MT tools/clang/lib/Sema/CMakeFiles/obj.clangSema.dir/SemaDecl.cpp.o \
         -MF tools/clang/lib/Sema/CMakeFiles/obj.clangSema.dir/SemaDecl.cpp.o.d \
         -o tools/clang/lib/Sema/CMakeFiles/obj.clangSema.dir/SemaDecl.cpp.o \
         -c $S/clang/lib/Sema/SemaDecl.cpp",
        "$S/clang/lib/Sema/SemaDecl.cpp",
        "/opt/llvm/bin/clang++ '-DCLANG_VENDOR=\"Mason \"' -D_DEBUG -D_GNU_SOURCE \
         -D__STDC_CONSTANT_MACROS -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS \
         -I$B/tools/clang/lib/Sema -I$S/clang/lib/Sema -I$S/clang/include \
         -I$B/tools/clang/include -I$B/include -I$S/llvm/include \
         -isystem /usr/include/libxml2 -fPIC -fvisibility-inlines-hidden \
         -Werror=date-time -Werror=unguarded-availability-new -Wall -Wextra \
         -Wno-unused-parameter -Wwrite-strings -Wcast-qual -Wmissing-field-initializers \
         -pedantic -Wno-long-long -Wc++98-compat-extra-semi -Wimplicit-fallthrough \
         -Wcovered-switch-default -Wno-noexcept-type -Wnon-virtual-dtor \
         -Wdelete-non-virtual-dtor -Wsuggest-override -Wstring-conversion \
         -Wmisleading-indentation -Wctad-maybe-unsupported \
         -ffunction-sections -fdata-sections -fno-common -Woverloaded-virtual \
         -Wno-nested-anon-types -O3 -DNDEBUG -std=c++17 -fno-exceptions -funwind-tables \
         -fno-rtti -UNDEBUG \
         $S/clang/lib/Sema/SemaDecl.cpp",
    );
}

#[test]
fn s5_llvm_libcxx_relative_input() {
    // A runtimes build: relative input, `-nostdinc++`, `--target=`, separate
    // `-isystem`, `-include`.
    let t = llvm_tree();
    llvm_case(
        &t,
        "/opt/llvm/bin/clang++ --target=x86_64-unknown-linux-gnu -DLIBCXX_BUILDING_LIBCXXABI \
         -D_LIBCPP_BUILDING_LIBRARY -D_LIBCPP_DISABLE_NEW_DELETE_DEFINITIONS \
         -D_LIBCPP_LINK_PTHREAD_LIB -D_LIBCPP_LINK_RT_LIB \
         -D_LIBCPP_REMOVE_TRANSITIVE_INCLUDES -I../src/libcxx/src \
         -isystem $B/include/c++/v1 -include $B/include/c++/v1/__config_site \
         -O3 -DNDEBUG -std=c++23 -fPIC -UNDEBUG -faligned-allocation -nostdinc++ \
         -fvisibility-inlines-hidden -fvisibility=hidden -fsized-deallocation \
         -Wall -Wextra -Wnewline-eof -Wshadow -Wwrite-strings -Wno-unused-parameter \
         -Wno-long-long -Werror=return-type -Wextra-semi -Wundef -Wunused-template \
         -Wformat-nonliteral -Wzero-length-array -Wdeprecated-redundant-constexpr-static-def \
         -Wno-user-defined-literals -Wno-covered-switch-default -Wno-suggest-override \
         -Wno-error -fdebug-prefix-map=$B/include/c++/v1=../src/libcxx/include \
         -MD -MT libcxx/src/CMakeFiles/cxx_objects.dir/string.cpp.o \
         -MF libcxx/src/CMakeFiles/cxx_objects.dir/string.cpp.o.d \
         -o libcxx/src/CMakeFiles/cxx_objects.dir/string.cpp.o \
         -c ../src/libcxx/src/string.cpp",
        "../src/libcxx/src/string.cpp",
        "/opt/llvm/bin/clang++ --target=x86_64-unknown-linux-gnu -DLIBCXX_BUILDING_LIBCXXABI \
         -D_LIBCPP_BUILDING_LIBRARY -D_LIBCPP_DISABLE_NEW_DELETE_DEFINITIONS \
         -D_LIBCPP_LINK_PTHREAD_LIB -D_LIBCPP_LINK_RT_LIB \
         -D_LIBCPP_REMOVE_TRANSITIVE_INCLUDES -I../src/libcxx/src \
         -isystem $B/include/c++/v1 -include $B/include/c++/v1/__config_site \
         -O3 -DNDEBUG -std=c++23 -fPIC -UNDEBUG -faligned-allocation -nostdinc++ \
         -fvisibility-inlines-hidden -fvisibility=hidden -fsized-deallocation \
         -Wall -Wextra -Wnewline-eof -Wshadow -Wwrite-strings -Wno-unused-parameter \
         -Wno-long-long -Werror=return-type -Wextra-semi -Wundef -Wunused-template \
         -Wformat-nonliteral -Wzero-length-array -Wdeprecated-redundant-constexpr-static-def \
         -Wno-user-defined-literals -Wno-covered-switch-default -Wno-suggest-override \
         -Wno-error -fdebug-prefix-map=$B/include/c++/v1=../src/libcxx/include \
         ../src/libcxx/src/string.cpp",
    );
}

#[test]
fn s5_llvm_tablegen_with_pch_and_xclang() {
    // CMake precompiled headers write `-Xclang -include-pch -Xclang <file>`
    // and `-Xclang -include -Xclang <header>`.
    let t = llvm_tree();
    llvm_case(
        &t,
        "/opt/llvm/bin/clang++ -D_DEBUG -D_GNU_SOURCE -D__STDC_CONSTANT_MACROS \
         -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS -I$B/utils/TableGen \
         -I$S/llvm/utils/TableGen -I$B/include -I$S/llvm/include \
         -fPIC -fno-color-diagnostics -std=c++17 -O2 -g -fno-exceptions -fno-rtti \
         -Xclang -include-pch -Xclang $B/utils/TableGen/CMakeFiles/llvm-tblgen.dir/cmake_pch.hxx.pch \
         -Xclang -include -Xclang $B/utils/TableGen/CMakeFiles/llvm-tblgen.dir/cmake_pch.hxx \
         -MD -MT utils/TableGen/CMakeFiles/llvm-tblgen.dir/TableGen.cpp.o \
         -MF utils/TableGen/CMakeFiles/llvm-tblgen.dir/TableGen.cpp.o.d \
         -o utils/TableGen/CMakeFiles/llvm-tblgen.dir/TableGen.cpp.o \
         -c $S/llvm/utils/TableGen/TableGen.cpp",
        "$S/llvm/utils/TableGen/TableGen.cpp",
        "/opt/llvm/bin/clang++ -D_DEBUG -D_GNU_SOURCE -D__STDC_CONSTANT_MACROS \
         -D__STDC_FORMAT_MACROS -D__STDC_LIMIT_MACROS -I$B/utils/TableGen \
         -I$S/llvm/utils/TableGen -I$B/include -I$S/llvm/include \
         -fPIC -std=c++17 -O2 -g -fno-exceptions -fno-rtti \
         -Xclang -include-pch -Xclang $B/utils/TableGen/CMakeFiles/llvm-tblgen.dir/cmake_pch.hxx.pch \
         -Xclang -include -Xclang $B/utils/TableGen/CMakeFiles/llvm-tblgen.dir/cmake_pch.hxx \
         $S/llvm/utils/TableGen/TableGen.cpp",
    );
}

#[test]
fn s5_llvm_ninja_response_file() {
    // Ninja with CMAKE_*_USE_RESPONSE_FILE_FOR_INCLUDES: includes in an rsp.
    let t = llvm_tree();
    t.write(
        "build/lib/Support/CMakeFiles/LLVMSupport.dir/includes_CXX.rsp",
        format!(
            "-I{b}/lib/Support -I{s}/llvm/lib/Support -isystem \"{b}/include\" -MD\n",
            b = t.abs("build"),
            s = t.abs("src")
        ),
    );
    llvm_case(
        &t,
        "/usr/bin/c++ -DNDEBUG @lib/Support/CMakeFiles/LLVMSupport.dir/includes_CXX.rsp \
         -std=c++17 -o lib/Support/CMakeFiles/LLVMSupport.dir/APFloat.cpp.o \
         -c $S/llvm/lib/Support/APFloat.cpp",
        "$S/llvm/lib/Support/APFloat.cpp",
        "/usr/bin/c++ -DNDEBUG -I$B/lib/Support -I$S/llvm/lib/Support -isystem $B/include \
         -std=c++17 $S/llvm/lib/Support/APFloat.cpp",
    );
}

// ---------------------------------------------------------------------------
// §4: response files.
// ---------------------------------------------------------------------------

fn expanded(t: &TempDir, args: &[&str]) -> Vec<String> {
    expand(t.path(), &strings(args)).unwrap_or_else(|e| panic!("expand({args:?}): {e:?}"))
}

fn expand_reason(t: &TempDir, args: &[&str]) -> Reason {
    match expand(t.path(), &strings(args)) {
        Ok(words) => panic!("expand({args:?}) succeeded: {words:?}"),
        Err(e) => e.reason,
    }
}

#[test]
fn s4_replaced_by_words() {
    let t = TempDir::new("rsp");
    t.write("a.rsp", "-DA \"-DB C\"\n-Iinc\n");
    assert_eq!(
        expanded(&t, &["clang", "-c", "@a.rsp", "x.c"]),
        ["clang", "-c", "-DA", "-DB C", "-Iinc", "x.c"]
    );
}

#[test]
fn s4_gnu_rules_apply_in_files() {
    let t = TempDir::new("rsp");
    t.write("a.rsp", r#""C:\dir\x" "a\"b""#);
    assert_eq!(expanded(&t, &["cc", "@a.rsp"]), ["cc", r"C:\dir\x", r#"a"b"#]);
}

#[test]
fn s4_argv0_not_expanded() {
    let t = TempDir::new("rsp");
    assert_eq!(expanded(&t, &["@cc", "x.c"]), ["@cc", "x.c"]);
}

#[test]
fn s4_absolute_path() {
    let t = TempDir::new("rsp");
    t.write("sub/a.rsp", "-DA");
    let at = format!("@{}", t.abs("sub/a.rsp"));
    assert_eq!(expanded(&t, &["cc", &at]), ["cc", "-DA"]);
}

#[test]
fn s4_nested_relative_to_directory() {
    // A nested `@path` is relative to the entry's directory, not to the
    // response file that names it.
    let t = TempDir::new("rsp");
    t.write("sub/outer.rsp", "-DOUTER @inner.rsp -DAFTER");
    t.write("inner.rsp", "-DINNER");
    t.write("sub/inner.rsp", "-DWRONG");
    assert_eq!(
        expanded(&t, &["cc", "@sub/outer.rsp", "x.c"]),
        ["cc", "-DOUTER", "-DINNER", "-DAFTER", "x.c"]
    );
}

#[test]
fn s4_depth_limit() {
    // `d1.rsp` names `d2.rsp` ... `dN.rsp`; the last holds a word.
    let t = TempDir::new("rsp");
    for depth in 1..=MAX_DEPTH + 1 {
        t.write(&format!("d{depth}.rsp"), format!("@d{}.rsp", depth + 1));
    }
    t.write(&format!("d{}.rsp", MAX_DEPTH + 2), "-DDEEP");
    // Depth exactly MAX_DEPTH is accepted.
    t.write(&format!("d{MAX_DEPTH}.rsp"), "-DDEEP");
    assert_eq!(expanded(&t, &["cc", "@d1.rsp"]), ["cc", "-DDEEP"]);
    // One more is not.
    t.write(&format!("d{MAX_DEPTH}.rsp"), format!("@d{}.rsp", MAX_DEPTH + 1));
    assert_eq!(expand_reason(&t, &["cc", "@d1.rsp"]), Reason::MalformedEntry);
    assert_eq!(MAX_DEPTH, 16);
}

#[test]
fn s4_cycle_is_too_deep() {
    let t = TempDir::new("rsp");
    t.write("a.rsp", "@b.rsp");
    t.write("b.rsp", "@a.rsp");
    let e = expand(t.path(), &strings(&["cc", "@a.rsp"])).unwrap_err();
    assert_eq!(e.reason, Reason::MalformedEntry);
    assert!(e.detail.contains("nested deeper than 16"), "{}", e.detail);
}

#[test]
fn s4_error_unreadable() {
    let t = TempDir::new("rsp");
    assert_eq!(expand_reason(&t, &["cc", "@missing.rsp"]), Reason::MalformedEntry);
    // A directory cannot be read as a file.
    std::fs::create_dir_all(t.path().join("dir.rsp")).expect("mkdir");
    assert_eq!(expand_reason(&t, &["cc", "@dir.rsp"]), Reason::MalformedEntry);
}

#[test]
fn s4_error_not_utf8() {
    let t = TempDir::new("rsp");
    t.write("a.rsp", b"-DA \xff\xfe");
    assert_eq!(expand_reason(&t, &["cc", "@a.rsp"]), Reason::MalformedEntry);
}

#[test]
fn s4_error_malformed_quoting() {
    let t = TempDir::new("rsp");
    t.write("a.rsp", "-DA \"unclosed");
    t.write("b.rsp", "-DA\\");
    assert_eq!(expand_reason(&t, &["cc", "@a.rsp"]), Reason::MalformedEntry);
    assert_eq!(expand_reason(&t, &["cc", "@b.rsp"]), Reason::MalformedEntry);
    // Nested, too.
    t.write("c.rsp", "@a.rsp");
    assert_eq!(expand_reason(&t, &["cc", "@c.rsp"]), Reason::MalformedEntry);
}

#[test]
fn s4_empty_file_contributes_nothing() {
    let t = TempDir::new("rsp");
    t.write("empty.rsp", "\n");
    assert_eq!(expanded(&t, &["cc", "@empty.rsp", "x.c"]), ["cc", "x.c"]);
}

#[test]
fn s4_bare_at_is_not_a_response_file() {
    let t = TempDir::new("rsp");
    assert_eq!(expanded(&t, &["cc", "@", "x.c"]), ["cc", "@", "x.c"]);
}

#[test]
fn s4_runs_before_normalisation() {
    // Words from a response file are classified like any others: `-o` and its
    // value split across the file boundary are removed together; a launcher
    // is not looked for in a response file since argv[0] is never expanded.
    let t = fixture();
    t.write("a.rsp", "-DA -o");
    assert_eq!(
        norm(&t, &["clang++", "@a.rsp", "main.o", "-c", "main.cpp"]),
        ["clang++", "-DA", "main.cpp"]
    );
    // An unreadable response file excludes the entry as malformed.
    assert_eq!(excluded(&t, &["clang++", "@gone.rsp", "main.cpp"]), Reason::MalformedEntry);
}
