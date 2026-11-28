//! `cargo xtask corpus` (spec/harness.md §8).
//!
//! The stamp tests run against fake `cmake` and `make` (shell scripts that
//! write a minimal build tree and log their calls). The end-to-end tests
//! fetch and configure a tiny local git repository holding a three-file
//! cmake project with one generated header; they need `git`, `cmake`,
//! `make` and a C++ compiler, and are reported `SKIP` with the reason
//! otherwise (an error under `SLIP_REQUIRE_ALL=1`, spec/harness.md §1).
//! No test uses the network.

#![cfg(unix)]

mod common;

use common::Fixture;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use xtask::corpus::{self, Corpus, Ctx, Done, Layout, rows};

const KILN: &str = "\
# Kiln's projects (a fixture).
[other]
url = \"https://example.org/other\"
tag = \"v2\"
build = \"meson\"

[tiny]
url = \"@URL@\"
tag = \"v1\"
build = \"cmake\"
cxx_std = 17
configure = [\"-DX=1\"]
verified = true
notes = \"a # in a string is not a comment\"

[archived]
url = \"https://example.org/a.tar.gz\"
tag = \"1.0\"
archive = true

[sub]
url = \"https://example.org/sub\"
tag = \"v3\"
source_subdir = \"llvm\"
";

fn row(name: &str, kiln: &str, configure: &str, generate: &str, cc: &str, cxx: &str) -> String {
    format!(
        "# comment\n[{name}]\nkiln = \"{kiln}\"\ncxx_std = 17\nbuild_type = \"Release\"\n\
         c_compiler = \"{cc}\"\ncxx_compiler = \"{cxx}\"\nconfigure = [{configure}]\n\
         generate = [\n  {generate},\n]\n"
    )
}

fn kiln(url: &str) -> String {
    KILN.replace("@URL@", url)
}

fn one(corpora: &str, url: &str) -> Corpus {
    let mut v = rows::parse(corpora, &kiln(url)).unwrap();
    assert_eq!(v.len(), 1);
    v.remove(0)
}

fn parse_err(corpora: &str) -> String {
    rows::parse(corpora, &kiln("https://example.org/tiny")).unwrap_err()
}

// ---------------------------------------------------------- corpora.toml

/// §8.1: a row with its pin taken from the Kiln table.
#[test]
fn parses_a_row_and_its_kiln_pin() {
    let c = one(
        &row("tiny", "tiny", "\"-DA=1\", \"-DB=x;y\"", "\"gen\", \"gen2\"", "cc", "c++"),
        "https://example.org/tiny",
    );
    assert_eq!(c.name, "tiny");
    assert_eq!(c.kiln, "tiny");
    assert_eq!(c.url, "https://example.org/tiny");
    assert_eq!(c.tag, "v1");
    assert_eq!(c.source_subdir, None);
    assert_eq!(c.cxx_std, 17);
    assert_eq!(c.build_type, "Release");
    assert_eq!((c.c_compiler.as_str(), c.cxx_compiler.as_str()), ("cc", "c++"));
    assert_eq!(c.configure, ["-DA=1", "-DB=x;y"]);
    assert_eq!(c.generate, ["gen", "gen2"]);

    let s = one(&row("s", "sub", "", "\"g\"", "cc", "c++"), "x");
    assert_eq!((s.url.as_str(), s.tag.as_str()), ("https://example.org/sub", "v3"));
    assert_eq!(s.source_subdir.as_deref(), Some("llvm"));
}

/// §8.1: every malformed row is an error naming it.
#[test]
fn rejects_malformed_rows() {
    let good = row("tiny", "tiny", "", "\"gen\"", "cc", "c++");
    let cases: Vec<(String, &str)> = vec![
        (row("tiny", "nope", "", "\"gen\"", "cc", "c++"), "names `nope`, which is not a table"),
        (row("tiny", "archived", "", "\"gen\"", "cc", "c++"), "must be a plain git clone"),
        (row("Tiny", "tiny", "", "\"gen\"", "cc", "c++"), "corpus name `Tiny`"),
        (row("tiny", "tiny", "\"-DCMAKE_BUILD_TYPE=Debug\"", "\"gen\"", "cc", "c++"), "xtask sets"),
        (row("tiny", "tiny", "\"-GNinja\"", "\"gen\"", "cc", "c++"), "xtask sets"),
        (row("tiny", "tiny", "\"-DCMAKE_CXX_COMPILER:STRING=g++\"", "\"g\"", "cc", "c"), "xtask"),
        (row("tiny", "tiny", "", "\"gen\", \"gen\"", "cc", "c++"), "lists `gen` twice"),
        (row("tiny", "tiny", "", "\"-j8\"", "cc", "c++"), "is not a build target name"),
        (row("tiny", "tiny", "", "\"t:a.o\"", "cc", "c++"), "or `<target>:<file>`"),
        (row("tiny", "tiny", "", "\"t:/abs.h\"", "cc", "c++"), "or `<target>:<file>`"),
        (row("tiny", "tiny", "", "\"t:../x.h\"", "cc", "c++"), "or `<target>:<file>`"),
        (row("tiny", "tiny", "", "\"gen\"", "/usr/bin/cc", "c++"), "`c_compiler` must be"),
        (row("tiny", "tiny", "1", "\"gen\"", "cc", "c++"), "must be an array of strings"),
        (good.replace("generate = [\n  \"gen\",\n]", "generate = []"), "`generate` is empty"),
        (good.replace("cxx_std = 17", "cxx_std = \"17\""), "non-negative integer"),
        (good.replace("cxx_std = 17\n", ""), "has no `cxx_std`"),
        (good.replace("kiln = \"tiny\"", "kiln = tiny"), "must be one string"),
        (format!("{good}extra = 1\n"), "unknown key `extra`"),
        (format!("{good}[tiny]\n"), "appears twice"),
        (format!("x = 1\n{good}"), "outside any table"),
        (format!("{good}[a.b]\n"), "unsupported table header"),
        (format!("{good}expected_unparsed = [\"bad\"]\n"), "must be `<outcome>:<path>:<reason>`"),
        (
            format!("{good}expected_unparsed = [\"ok:a.cpp:why\"]\n"),
            "must be `diagnosed`, `rejected` or `failed`",
        ),
        (
            format!("{good}expected_unparsed = [\"diagnosed:/abs.cpp:why\"]\n"),
            "is not a relative path under `src/`",
        ),
        (
            format!("{good}expected_unparsed = [\"diagnosed:../a.cpp:why\"]\n"),
            "is not a relative path under `src/`",
        ),
        (
            format!("{good}expected_unparsed = [\"diagnosed:a.cpp:   \"]\n"),
            "needs a one-line reason",
        ),
        (
            format!("{good}expected_unparsed = [\"diagnosed:a.cpp:r1\", \"failed:a.cpp:r2\"]\n"),
            "lists `a.cpp` twice",
        ),
        (format!("{good}expected_excluded = [\"bad\"]\n"), "must be `<reason>:<path>:<note>`"),
        (
            format!("{good}expected_excluded = [\"bogus:a.cpp:why\"]\n"),
            "must be one of not_a_compile",
        ),
        (
            format!("{good}expected_excluded = [\"unsupported_flag:/abs.cpp:why\"]\n"),
            "is not a relative path under `src/`",
        ),
        (
            format!("{good}expected_excluded = [\"unsupported_flag:../a.cpp:why\"]\n"),
            "is not a relative path under `src/`",
        ),
        (
            format!("{good}expected_excluded = [\"unsupported_flag:a.cpp:   \"]\n"),
            "needs a one-line note",
        ),
        (
            format!(
                "{good}expected_excluded = [\"unsupported_flag:a.cpp:n1\", \"not_a_compile:a.cpp:n2\"]\n"
            ),
            "lists `a.cpp` twice",
        ),
    ];
    for (text, want) in cases {
        let e = parse_err(&text);
        assert!(e.contains(want), "{text}\n=> {e}\nwanted `{want}`");
        assert!(e.starts_with("bench/corpora.toml") || e.starts_with("kiln/scripts/oss/"), "{e}");
    }
    // A Kiln row without a tag is an error naming Kiln's file.
    let e = rows::parse(&good, &kiln("u").replace("tag = \"v1\"\n", "")).unwrap_err();
    assert!(e.contains("kiln/scripts/oss/projects.toml: [tiny] has no `tag`"), "{e}");
}

/// The committed `bench/corpora.toml` reads, and its `llvm` row takes
/// llvm-project 19.1.7 from Kiln's `llvm_project` (§8.7).
#[test]
fn the_committed_llvm_row() {
    let repo = xtask::repo_root();
    let all = rows::load(&repo).unwrap();
    let c = rows::find(&all, "llvm").unwrap();
    assert_eq!(c.kiln, "llvm_project");
    assert_eq!(c.url, "https://github.com/llvm/llvm-project");
    assert_eq!(c.tag, "llvmorg-19.1.7");
    assert_eq!(c.source_subdir.as_deref(), Some("llvm"));
    assert_eq!(c.cxx_std, 17);
    assert_eq!((c.c_compiler.as_str(), c.cxx_compiler.as_str()), ("clang-19", "clang++-19"));
    assert!(c.configure.iter().any(|a| a == "-DLLVM_ENABLE_PROJECTS=clang"));
    assert!(c.configure.iter().any(|a| a == "-DLLVM_TARGETS_TO_BUILD=X86;AArch64"));
    for t in ["intrinsics_gen", "X86CommonTableGen", "AArch64CommonTableGen"] {
        assert!(c.generate.iter().any(|g| g == t), "{t}");
    }
    let unparsed: Vec<(&str, slip_model::Status)> =
        c.expected_unparsed.iter().map(|e| (e.path.as_str(), e.status)).collect();
    assert_eq!(
        unparsed,
        [
            ("clang/tools/clang-fuzzer/ClangFuzzer.cpp", slip_model::Status::Diagnosed),
            ("clang/tools/clang-fuzzer/ClangObjectiveCFuzzer.cpp", slip_model::Status::Diagnosed),
        ]
    );
    assert!(c.expected_unparsed.iter().all(|e| !e.reason.is_empty()));
    assert!(c.expected_excluded.is_empty());
    assert!(rows::find(&all, "nope").unwrap_err().contains("corpora: llvm"));
}

// ---------------------------------------------------------- layout

/// §8.2: the root is `$SLIP_CORPUS_DIR`, else `.corpus/`.
#[test]
fn corpus_root_and_layout() {
    let repo = Path::new("/repo");
    assert_eq!(corpus::corpus_root(repo, None).unwrap(), Path::new("/repo/.corpus"));
    assert_eq!(corpus::corpus_root(repo, Some("".as_ref())).unwrap(), Path::new("/repo/.corpus"));
    assert_eq!(corpus::corpus_root(repo, Some("/c".as_ref())).unwrap(), Path::new("/c"));
    let e = corpus::corpus_root(repo, Some("rel/dir".as_ref())).unwrap_err();
    assert!(e.contains("must be an absolute path"), "{e}");

    let l = Layout::new(Path::new("/c"), "llvm");
    assert_eq!(l.dir, Path::new("/c/llvm"));
    assert_eq!(l.src, Path::new("/c/llvm/src"));
    assert_eq!(l.build, Path::new("/c/llvm/build"));
    assert_eq!(l.database(), Path::new("/c/llvm/build/compile_commands.json"));
    assert_eq!(l.fetch_stamp(), Path::new("/c/llvm/fetch.stamp"));
    let c = one(&row("s", "sub", "", "\"g\"", "cc", "c++"), "x");
    assert_eq!(l.source_dir(&c), Path::new("/c/llvm/src/llvm"));
}

/// The binary: `path` prints the directory; `path --db` fails while there
/// is no database; usage errors exit 2.
#[test]
fn path_command_and_usage() {
    let f = Fixture::empty("corpus-path");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("corpus")
            .args(args)
            .env("SLIP_CORPUS_DIR", f.root())
            .output()
            .unwrap()
    };
    let out = run(&["path", "llvm"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), f.path("llvm").display().to_string());
    let out = run(&["path", "llvm", "--db"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("corpus configure llvm"));
    for bad in [&["frob"][..], &["fetch"], &["status", "a", "b"], &["path", "llvm", "--x"], &[]] {
        assert_eq!(run(bad).status.code(), Some(2), "{bad:?}");
    }
    assert_eq!(run(&["status", "nope"]).status.code(), Some(1));
    let out = run(&["list"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("llvm  llvm_project  llvmorg-19.1.7"));
}

// ---------------------------------------------------------- missing tools

/// §8.3: a missing tool fails with one line naming it and
/// `scripts/provision.sh --full`.
#[test]
fn missing_tool_message() {
    let f = Fixture::empty("corpus-tools");
    std::fs::create_dir_all(f.path("empty")).unwrap();
    let path = Some(OsString::from(f.path("empty")));
    let e = corpus::find_tool("cmake", path.as_deref()).unwrap_err();
    assert_eq!(
        e,
        "`cmake` is not on PATH: run scripts/provision.sh --full (nothing is installed by xtask)"
    );
    assert!(!e.contains('\n'));
    // A file that is not executable is not the tool.
    f.write("empty/cmake", "");
    assert!(corpus::find_tool("cmake", path.as_deref()).is_err());

    let c = one(&row("tiny", "tiny", "", "\"gen\"", "cc", "c++"), "https://example.org/tiny");
    let mut out = Vec::new();
    let mut ctx = Ctx { layout: Layout::new(&f.path("root"), "tiny"), path, out: &mut out };
    let e = corpus::fetch(&c, &mut ctx).unwrap_err();
    assert!(e.starts_with("`git` is not on PATH: run scripts/provision.sh --full"), "{e}");
    // The lock is released after a failure.
    assert!(!ctx.layout.lock().exists());
}

// ---------------------------------------------------------- stamps (fakes)

/// A corpus directory whose source is already fetched (a stamped `src`),
/// and fake tools in `bin/`: `cmake` writes a build tree with one target
/// `gen` promising `sub/gen.inc`, `make` makes it; both log to `log`.
struct Fake {
    f: Fixture,
}

impl Fake {
    fn new(tag: &str, c: &Corpus) -> Fake {
        let f = Fixture::empty(tag);
        let log = f.path("log");
        let cmake = format!(
            "#!/bin/sh\n[ \"$1\" = --probe ] && exit 0\necho \"cmake $*\" >> '{log}'\n\
             while [ $# -gt 0 ]; do case \"$1\" in -B) shift; B=\"$1\";; esac; shift; done\n\
             mkdir -p \"$B/CMakeFiles\" \"$B/sub/CMakeFiles/gen.dir\" \"$B/sub/CMakeFiles/dep.dir\"\n\
             printf 'gen: sub/CMakeFiles/gen.dir/rule\\ndep: sub/CMakeFiles/dep.dir/rule\\n\
             sub/CMakeFiles/gen.dir/all: sub/CMakeFiles/dep.dir/all\\n' > \"$B/CMakeFiles/Makefile2\"\n\
             printf 'gen: sub/CMakeFiles/gen\\ngen: sub/gen.inc\\nsub/extra.h: sub/gen.inc\\n\\tcmd\\n' > \"$B/sub/CMakeFiles/gen.dir/build.make\"\n\
             printf 'dep: sub/dep.def sub/CMakeFiles/dep.dir/x.o\\n' > \"$B/sub/CMakeFiles/dep.dir/build.make\"\n\
             printf '[{{\"file\": \"a.cpp\"}}, {{\"file\": \"b.cpp\"}}]' > \"$B/compile_commands.json\"\n",
            log = log.display()
        );
        let make = format!(
            "#!/bin/sh\n[ \"$1\" = --probe ] && exit 0\necho \"make $*\" >> '{log}'\nmkdir -p sub\ntouch sub/gen.inc sub/dep.def\n\
             if [ \"$1\" = -f ]; then touch \"$3\"; fi\n",
            log = log.display()
        );
        for (name, text) in [
            ("bin/cmake", cmake.as_str()),
            ("bin/make", make.as_str()),
            ("bin/fakecc", "#!/bin/sh\n"),
            ("bin/fakecxx", "#!/bin/sh\n"),
        ] {
            f.write(name, text);
            std::fs::set_permissions(f.path(name), std::fs::Permissions::from_mode(0o755)).unwrap();
            common::wait_executable(&f.path(name));
        }
        let fake = Fake { f };
        let l = fake.layout(c);
        std::fs::create_dir_all(&l.src).unwrap();
        std::fs::write(l.fetch_stamp(), corpus::fetch_stamp(c, "0123abcd")).unwrap();
        fake
    }
    fn layout(&self, c: &Corpus) -> Layout {
        Layout::new(&self.f.path("root"), &c.name)
    }
    fn configure(&self, c: &Corpus) -> Result<corpus::Configured, String> {
        let mut out = Vec::new();
        let mut ctx = Ctx {
            layout: self.layout(c),
            path: Some(OsString::from(self.f.path("bin"))),
            out: &mut out,
        };
        corpus::configure(c, &mut ctx)
    }
    /// The logged calls since the last `take`, whole.
    fn take_lines(&self) -> Vec<String> {
        let text = std::fs::read_to_string(self.f.path("log")).unwrap_or_default();
        let _ = std::fs::remove_file(self.f.path("log"));
        text.lines().map(str::to_owned).collect()
    }
    /// The logged calls since the last `take`.
    fn take(&self) -> Vec<String> {
        let text = std::fs::read_to_string(self.f.path("log")).unwrap_or_default();
        let _ = std::fs::remove_file(self.f.path("log"));
        text.lines().map(|l| l.split(' ').next().unwrap_or_default().to_owned()).collect()
    }
}

fn fake_corpus(configure: &str, generate: &str) -> Corpus {
    one(&row("tiny", "tiny", configure, generate, "fakecc", "fakecxx"), "https://example.org/t")
}

/// §8.4: an unchanged row is a no-op; a changed configuration
/// runs cmake and the generate step again; a changed target list runs
/// only the generate step.
#[test]
fn stamps_make_steps_idempotent() {
    let c = fake_corpus("\"-DA=1\"", "\"gen\"");
    let fake = Fake::new("corpus-stamps", &c);
    let r = fake.configure(&c).unwrap();
    assert_eq!((r.cmake, r.generate, r.entries, r.generated), (Done::Ran, Done::Ran, 2, 2));
    assert_eq!(fake.take(), ["cmake", "make"]);
    let l = fake.layout(&c);
    let stamp = std::fs::read_to_string(l.configure_stamp()).unwrap();
    assert!(stamp.contains("commit 0123abcd\n") && stamp.contains("arg -DA=1\n"), "{stamp}");
    assert!(stamp.contains("arg -DCMAKE_EXPORT_COMPILE_COMMANDS=ON\n"), "{stamp}");

    let r = fake.configure(&c).unwrap();
    assert_eq!((r.cmake, r.generate), (Done::Current, Done::Current));
    assert!(fake.take().is_empty());

    let changed = fake_corpus("\"-DA=2\"", "\"gen\"");
    let r = fake.configure(&changed).unwrap();
    assert_eq!((r.cmake, r.generate), (Done::Ran, Done::Ran));
    assert_eq!(fake.take(), ["cmake", "make"]);

    let more = fake_corpus("\"-DA=2\"", "\"gen\", \"dep\"");
    let r = fake.configure(&more).unwrap();
    assert_eq!((r.cmake, r.generate), (Done::Current, Done::Ran));
    assert_eq!(fake.take(), ["make"]);

    // A missing database makes cmake run again even with a current stamp.
    std::fs::remove_file(l.database()).unwrap();
    assert_eq!(fake.configure(&more).unwrap().cmake, Done::Ran);

    let s = corpus::status(&more, &l);
    assert_eq!(s.source.as_deref(), Some("0123abcd"));
    assert!(s.build && s.configured && s.generated);
    assert_eq!(s.database, Some(Ok(2)));
    let text = corpus::render_status(&more, &l, &s);
    assert!(text.contains("(2 entries)") && text.contains("generated done (2 targets)"), "{text}");
    // The status of a row with another target list is "not done".
    assert!(!corpus::status(&changed, &l).generated);
}

/// §8.6: a promised file that is missing fails
/// configure and leaves no generate stamp; an unknown target is an error.
#[test]
fn generated_check() {
    let c = fake_corpus("", "\"gen\"");
    let fake = Fake::new("corpus-gencheck", &c);
    fake.configure(&c).unwrap();
    let l = fake.layout(&c);
    // `dep` is a dependency of `gen`: its outputs are promised too, its
    // object file (under CMakeFiles/) is not.
    std::fs::remove_file(l.build.join("sub/dep.def")).unwrap();
    let e = fake.configure(&c).unwrap_err();
    assert!(e.contains("1 of the 2 generated files"), "{e}");
    assert!(!l.generate_stamp().exists());
    // The next run builds the targets again.
    fake.take();
    fake.configure(&c).unwrap();
    assert_eq!(fake.take(), ["make"]);

    // A `<target>:<file>` entry: one rule of the target's build.make.
    let file = fake_corpus("", "\"gen\", \"gen:sub/extra.h\"");
    fake.take();
    let done = fake.configure(&file).unwrap();
    assert_eq!(done.generated, 3);
    assert_eq!(
        fake.take_lines(),
        ["make gen", "make -f sub/CMakeFiles/gen.dir/build.make sub/extra.h"]
    );
    assert!(l.build.join("sub/extra.h").is_file());
    let norule = fake_corpus("", "\"gen:sub/none.h\"");
    let e = fake.configure(&norule).unwrap_err();
    assert!(e.contains("`sub/none.h` has no rule in sub/CMakeFiles/gen.dir/build.make"), "{e}");

    let unknown = fake_corpus("", "\"nope\"");
    let e = fake.configure(&unknown).unwrap_err();
    assert!(e.contains("target `nope` is not in the configured build"), "{e}");
}

/// §8.5: a step fails while a live process holds the lock and
/// takes over a lock whose process is gone.
#[test]
fn lock() {
    let c = fake_corpus("", "\"gen\"");
    let fake = Fake::new("corpus-lock", &c);
    let l = fake.layout(&c);
    std::fs::write(l.lock(), format!("{}\n", std::process::id())).unwrap();
    let e = fake.configure(&c).unwrap_err();
    assert!(e.contains("is held by process"), "{e}");
    assert!(fake.take().is_empty());
    std::fs::write(l.lock(), "4294967295\n").unwrap();
    fake.configure(&c).unwrap();
    assert!(!l.lock().exists());
}

// ---------------------------------------------------------- end to end

/// The tools the end-to-end tests need, or the reason they are skipped.
fn real_tools() -> Result<(String, String), String> {
    let path = std::env::var_os("PATH");
    for t in ["git", "cmake", "make"] {
        corpus::find_tool(t, path.as_deref())?;
    }
    for (cc, cxx) in [("clang-19", "clang++-19"), ("cc", "c++")] {
        if corpus::find_tool(cc, path.as_deref()).is_ok()
            && corpus::find_tool(cxx, path.as_deref()).is_ok()
        {
            return Ok((cc.to_owned(), cxx.to_owned()));
        }
    }
    Err("no C and C++ compiler (clang-19/clang++-19 or cc/c++) on PATH".to_owned())
}

fn skip(test: &str, reason: &str) {
    assert!(
        std::env::var_os("SLIP_REQUIRE_ALL").is_none_or(|v| v != "1"),
        "{test}: required by SLIP_REQUIRE_ALL=1 but cannot run: {reason}"
    );
    eprintln!("SKIP {test}: {reason}");
}

/// A git repository at `dir` with a three-file cmake project (one
/// generated header, made by target `gen`), tagged `v1`.
fn tiny_repo(dir: &Path) {
    let files = [
        (
            "CMakeLists.txt",
            "cmake_minimum_required(VERSION 3.20)\nproject(tiny C CXX)\n\
             add_custom_command(OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/gen.inc\n\
             \x20 COMMAND ${CMAKE_COMMAND} -E copy ${CMAKE_CURRENT_SOURCE_DIR}/gen.txt gen.inc\n\
             \x20 DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/gen.txt)\n\
             add_custom_target(gen DEPENDS ${CMAKE_CURRENT_BINARY_DIR}/gen.inc)\n\
             add_executable(tiny main.cpp)\n\
             target_include_directories(tiny PRIVATE ${CMAKE_CURRENT_BINARY_DIR})\n\
             add_dependencies(tiny gen)\n",
        ),
        ("main.cpp", "#include \"gen.inc\"\nint main() { return VALUE - 1; }\n"),
        ("gen.txt", "#define VALUE 1\n"),
    ];
    std::fs::create_dir_all(dir).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let git = |args: &[&str]| {
        let st = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.org",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(st.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&st.stderr));
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "tiny"]);
    git(&["tag", "v1"]);
    std::fs::write(dir.join("gen.txt"), "#define VALUE 2\n").unwrap();
    git(&["commit", "-q", "-am", "after the tag"]);
}

/// §8.3: fetch and configure against a local repository.
#[test]
fn fetch_and_configure_a_local_project() {
    let (cc, cxx) = match real_tools() {
        Ok(t) => t,
        Err(reason) => return skip("fetch_and_configure_a_local_project", &reason),
    };
    let fx = Fixture::empty("corpus-e2e");
    tiny_repo(&fx.path("upstream"));
    let url = fx.path("upstream").display().to_string();
    let c = one(&row("tiny", "tiny", "", "\"gen\"", &cc, &cxx), &url);
    let layout = Layout::new(&fx.path("root"), "tiny");
    let mut out = Vec::new();
    let mut ctx = Ctx { layout: layout.clone(), path: std::env::var_os("PATH"), out: &mut out };

    assert_eq!(corpus::fetch(&c, &mut ctx).unwrap(), Done::Ran);
    // The clone is at the tag, not at the branch head after it.
    let text = std::fs::read_to_string(layout.src.join("gen.txt")).unwrap();
    assert_eq!(text, "#define VALUE 1\n");
    assert!(!fx.path("root/tiny/src.partial").exists());
    assert_eq!(corpus::fetch(&c, &mut ctx).unwrap(), Done::Current);

    let done = corpus::configure(&c, &mut ctx).unwrap();
    assert_eq!(
        (done.cmake, done.generate, done.entries, done.generated),
        (Done::Ran, Done::Ran, 1, 1)
    );
    assert_eq!(std::fs::read_to_string(layout.build.join("gen.inc")).unwrap(), text);
    let db = std::fs::read_to_string(layout.database()).unwrap();
    assert!(db.contains("main.cpp"), "{db}");
    let done = corpus::configure(&c, &mut ctx).unwrap();
    assert_eq!((done.cmake, done.generate), (Done::Current, Done::Current));
    let st = corpus::status(&c, &layout);
    assert!(st.source.is_some() && st.configured && st.generated);
    assert_eq!(st.database, Some(Ok(1)));

    // A clone whose stamp is lost is verified in place and stamped again.
    std::fs::remove_file(layout.fetch_stamp()).unwrap();
    assert_eq!(corpus::fetch(&c, &mut ctx).unwrap(), Done::Ran);
    assert!(corpus::status(&c, &layout).source.is_some());
    // That invalidated the later steps.
    assert!(!layout.configure_stamp().exists());

    // A row pinning another tag does not reuse or overwrite the clone.
    let kiln2 = kiln(&url).replace("tag = \"v1\"", "tag = \"v9\"");
    let mut v = rows::parse(&row("tiny", "tiny", "", "\"gen\"", &cc, &cxx), &kiln2).unwrap();
    let other = v.remove(0);
    let e = corpus::fetch(&other, &mut ctx).unwrap_err();
    assert!(e.contains("has no tag `v9`") && e.contains("remove it to fetch again"), "{e}");
}

/// §8.3 fetch: a tag the repository does not have fails and leaves no
/// `src`.
#[test]
fn fetch_of_a_missing_tag() {
    if let Err(reason) = real_tools() {
        return skip("fetch_of_a_missing_tag", &reason);
    }
    let f = Fixture::empty("corpus-notag");
    tiny_repo(&f.path("upstream"));
    let kiln2 = kiln(&f.path("upstream").display().to_string()).replace("\"v1\"", "\"v9\"");
    let c =
        rows::parse(&row("tiny", "tiny", "", "\"gen\"", "cc", "c++"), &kiln2).unwrap().remove(0);
    let layout = Layout::new(&f.path("root"), "tiny");
    let mut out = Vec::new();
    let mut ctx = Ctx { layout: layout.clone(), path: std::env::var_os("PATH"), out: &mut out };
    let e = corpus::fetch(&c, &mut ctx).unwrap_err();
    assert!(e.contains("git clone failed"), "{e}");
    assert!(!layout.src.exists() && !layout.fetch_stamp().exists());
}
