//! `slip_kiln::snippet`, the in-memory front end for tests
//! (`docs/wp/WP-021.md`, "Interface"; every test below covers that
//! section).

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::dump::{DumpOptions, dump};
use slip_kiln::kiln::source::FileId;
use slip_kiln::snippet::{Parsed, Snippet, parse};

/// A snippet of `files` with main file `main` and no include directory, in
/// C++20.
fn snippet<'a>(files: &'a [(&'a str, &'a str)], main: &'a str) -> Snippet<'a> {
    Snippet { files, main, include: &[], std: "c++20" }
}

/// The top-level declarations written in a snippet file, as
/// `(name, kind, path of the file)`, in order.
fn top_level(p: &Parsed<'_>) -> Vec<(String, DeclKind, Option<String>)> {
    let ast = p.ast;
    ast.members(ast.tu)
        .filter_map(|d| {
            let node = ast.decls.get(d);
            let name = node.name().as_ident()?.as_str(ast.symbols()).to_owned();
            let file = file_of(p, node.loc())?;
            let path = p.path(file).map(str::to_owned);
            path.as_ref()?;
            Some((name, node.kind(), path))
        })
        .collect()
}

/// The buffer a location is spelled in.
fn file_of(p: &Parsed<'_>, loc: slip_kiln::kiln::source::Loc) -> Option<FileId> {
    p.sm.offset_of(p.sm.spelling_loc(loc)).map(|(f, _)| f)
}

fn names(p: &Parsed<'_>) -> Vec<String> {
    top_level(p).into_iter().map(|(n, _, _)| n).collect()
}

/// The AST dump of the main file, then the diagnostics.
fn fingerprint(p: &Parsed<'_>) -> String {
    let opts =
        DumpOptions { sources: Some(p.sm), main_file: Some(p.main_file), ..Default::default() };
    let mut out = Vec::new();
    dump(p.ast, &opts, &mut out).expect("a dump to memory");
    let mut s = String::from_utf8(out).expect("a UTF-8 dump");
    s.push_str("-- diagnostics --\n");
    s.push_str(&p.diagnostics);
    s
}

// One file: the callback sees its declarations; no error.
#[test]
fn wp021_one_file() {
    let files = [("main.cpp", "int f(int x) { return x + 1; }\nstruct S { int a; };\n")];
    let got = parse(&snippet(&files, "main.cpp"), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        assert_eq!(p.warnings, 0, "{}", p.diagnostics);
        assert_eq!(p.diagnostics, "");
        assert_eq!(p.path(p.main_file), Some("main.cpp"));
        top_level(p)
    })
    .unwrap();
    let f = got.iter().find(|(n, _, _)| n == "f").expect("the function `f`");
    assert_eq!(f.1, DeclKind::Function);
    assert_eq!(f.2.as_deref(), Some("main.cpp"));
    assert!(got.iter().any(|(n, _, _)| n == "S"), "{got:?}");
}

// Quote includes look beside the includer, then through `include` in
// order; angle includes only through `include`; `path` of each buffer.
#[test]
fn wp021_include_search() {
    let files = [
        (
            "src/main.cpp",
            "#include \"beside.h\"\n#include \"sub/deeper.h\"\n#include <c.h>\n\
             #include \"d.h\"\n#include \"../up.h\"\nint in_main;\n",
        ),
        ("src/beside.h", "int from_beside;\n"),
        ("src/sub/deeper.h", "#include \"next.h\"\nint from_deeper;\n"),
        ("src/sub/next.h", "int from_next;\n"),
        ("up.h", "int from_up;\n"),
        ("inc1/c.h", "int from_inc1;\n"),
        ("inc2/c.h", "int from_inc2;\n"),
        ("inc2/d.h", "int from_d;\n"),
        // Never included: a quote include in main of "c.h" would not see it.
        ("c.h", "int from_root;\n"),
    ];
    let s =
        Snippet { files: &files, main: "src/main.cpp", include: &["inc1", "inc2"], std: "c++17" };
    let got = parse(&s, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        top_level(p)
    })
    .unwrap();
    let expect = [
        ("from_beside", "src/beside.h"),
        ("from_next", "src/sub/next.h"),
        ("from_deeper", "src/sub/deeper.h"),
        ("from_inc1", "inc1/c.h"),
        ("from_d", "inc2/d.h"),
        ("from_up", "up.h"),
        ("in_main", "src/main.cpp"),
    ];
    let got: Vec<(&str, &str)> =
        got.iter().map(|(n, _, f)| (n.as_str(), f.as_deref().unwrap())).collect();
    assert_eq!(got, expect);
}

// The root is an include directory spelled `""`; `#include_next` goes on
// after the directory the includer was found in.
#[test]
fn wp021_root_and_include_next() {
    let files = [
        ("main.cpp", "#include <a.h>\n"),
        ("a.h", "#include_next <a.h>\nint first;\n"),
        ("lib/a.h", "int second;\n"),
    ];
    let s = Snippet { files: &files, main: "main.cpp", include: &["", "lib"], std: "c++20" };
    let got = parse(&s, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        names(p)
    })
    .unwrap();
    assert_eq!(got, ["second", "first"]);
}

// An angle include that only the includer's directory could satisfy is
// not found.
#[test]
fn wp021_angle_include_not_beside() {
    let files = [("x/main.cpp", "#include <y.h>\nint after;\n"), ("x/y.h", "int y;\n")];
    let s = Snippet { files: &files, main: "x/main.cpp", include: &[], std: "c++20" };
    let (errors, diags) = parse(&s, |p| (p.errors, p.diagnostics.clone())).unwrap();
    assert!(errors >= 1);
    assert!(diags.contains("x/main.cpp:1:10: fatal error: 'y.h' file not found"), "{diags}");
}

// `#pragma once` and an include guard: each header's declarations once.
#[test]
fn wp021_once_and_guard() {
    let files = [
        (
            "main.cpp",
            "#include \"once.h\"\n#include \"guard.h\"\n#include \"once.h\"\n#include \"guard.h\"\n",
        ),
        ("once.h", "#pragma once\nstruct Once {};\n"),
        ("guard.h", "#ifndef GUARD_H\n#define GUARD_H\nstruct Guard {};\n#endif\n"),
    ];
    let got = parse(&snippet(&files, "main.cpp"), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        names(p)
    })
    .unwrap();
    assert_eq!(got, ["Once", "Guard"]);
}

// A buffer the front end made itself (the scratch space of `##`) has no
// snippet path.
#[test]
fn wp021_path_of_a_made_buffer() {
    let files = [("main.cpp", "#define CAT(a, b) a##b\nint CAT(pas, ted);\n")];
    parse(&snippet(&files, "main.cpp"), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let ast = p.ast;
        let d = ast
            .members(ast.tu)
            .find(|&d| {
                ast.decls
                    .get(d)
                    .name()
                    .as_ident()
                    .is_some_and(|s| s.as_str(ast.symbols()) == "pasted")
            })
            .expect("the declaration `pasted`");
        let file = file_of(p, ast.decls.get(d).loc()).expect("a spelling buffer");
        assert_ne!(file, p.main_file);
        assert_eq!(p.path(file), None, "{}", p.sm.file_name(file));
    })
    .unwrap();
}

// A missing include: an error with Kiln's message, and the callback is
// still called.
#[test]
fn wp021_missing_include() {
    let files = [("main.cpp", "int before;\n#include \"absent.h\"\n")];
    let (errors, diags) =
        parse(&snippet(&files, "main.cpp"), |p| (p.errors, p.diagnostics.clone())).unwrap();
    assert!(errors >= 1);
    assert!(diags.contains("main.cpp:2:10: fatal error: 'absent.h' file not found"), "{diags}");
}

// A semantic error: one error, rendered with the snippet path, the line
// and the column.
#[test]
fn wp021_semantic_error() {
    let files = [("lib/a.cpp", "int ok;\nint f() { return missing; }\n")];
    let (errors, diags, seen) =
        parse(&snippet(&files, "lib/a.cpp"), |p| (p.errors, p.diagnostics.clone(), names(p)))
            .unwrap();
    assert_eq!(errors, 1, "{diags}");
    assert!(
        diags.starts_with("lib/a.cpp:2:18: error: use of undeclared identifier 'missing'\n"),
        "{diags}"
    );
    assert!(seen.contains(&"f".to_owned()), "{seen:?}");
}

// Each standard; an unknown one is an error.
#[test]
fn wp021_standards() {
    // `char8_t` from C++20, the `z` literal suffix from C++23 (a warning
    // before), pack indexing from C++26 (a warning before): each line is
    // clean only from its standard on.
    for (std, cxx20, cxx23, cxx26) in [
        ("c++17", false, false, false),
        ("c++20", true, false, false),
        ("c++23", true, true, false),
        ("c++26", true, true, true),
    ] {
        let clean = |text: &str| {
            let files = [("main.cpp", text)];
            let s = Snippet { files: &files, main: "main.cpp", include: &[], std };
            parse(&s, |p| p.errors + p.warnings == 0).unwrap()
        };
        assert!(clean("int x;\n"), "{std}");
        assert_eq!(clean("char8_t c;\n"), cxx20, "{std}");
        assert_eq!(clean("auto z = 1z;\n"), cxx23, "{std}");
        assert_eq!(clean("template <class... T> using F = T...[0];\n"), cxx26, "{std}");
    }
    let files = [("main.cpp", "")];
    for bad in ["c++14", "gnu++20", "c11", "c++2c", ""] {
        let s = Snippet { files: &files, main: "main.cpp", include: &[], std: bad };
        let err = parse(&s, |_| ()).unwrap_err();
        assert!(err.contains(&format!("unknown std `{bad}`")), "{err}");
    }
}

// A wrong snippet: a path twice, `main` not among the files, a path or an
// include directory that is not normalised.
#[test]
fn wp021_wrong_snippets() {
    let twice = [("a.cpp", ""), ("a.cpp", "int x;")];
    let err = parse(&snippet(&twice, "a.cpp"), |_| ()).unwrap_err();
    assert_eq!(err, "snippet path `a.cpp` is given twice");

    let files = [("a.cpp", "")];
    let err = parse(&snippet(&files, "b.cpp"), |_| ()).unwrap_err();
    assert_eq!(err, "the main file `b.cpp` is not among the snippet's files");

    for bad in ["/a.cpp", "x/../a.cpp", "./a.cpp", "x//a.cpp", "a/", ""] {
        let files = [(bad, "")];
        let err = parse(&snippet(&files, bad), |_| ()).unwrap_err();
        assert_eq!(err, format!("snippet path `{bad}` is not a normalised relative path"));
    }
    let s = Snippet { files: &files, main: "a.cpp", include: &["", "inc/"], std: "c++20" };
    let err = parse(&s, |_| ()).unwrap_err();
    assert_eq!(err, "include directory `inc/` is not a normalised relative path");
}

// A panic in the callback is an `Err` with its message; the next call
// works.
#[test]
fn wp021_panic_in_callback() {
    let files = [("main.cpp", "int x;\n")];
    let s = snippet(&files, "main.cpp");
    let err = parse(&s, |_| -> () { panic!("boom {}", 42) }).unwrap_err();
    assert_eq!(err, "the callback panicked: boom 42");
    assert_eq!(parse(&s, names).unwrap(), ["x"]);
}

// Sixteen threads, each parsing its own snippet a hundred times: every
// result is that of the snippet parsed alone.
#[test]
fn wp021_threads() {
    const THREADS: usize = 16;
    const ROUNDS: usize = 100;
    let texts: Vec<(String, String)> = (0..THREADS)
        .map(|i| {
            let header =
                format!("#pragma once\ntemplate <class T> T twice_{i}(T t) {{ return t + t; }}\n");
            let main = format!(
                "#include \"h{i}.h\"\nnamespace n{i} {{ struct S{i} {{ int v = {i}; }}; }}\n\
                 int use_{i}() {{ return twice_{i}({i}) + n{i}::S{i}{{}}.v; }}\n\
                 int bad_{i}() {{ return undeclared_{i}; }}\n"
            );
            (header, main)
        })
        .collect();
    let alone: Vec<String> = (0..THREADS)
        .map(|i| {
            let hname = format!("h{i}.h");
            let files = [("main.cpp", texts[i].1.as_str()), (hname.as_str(), texts[i].0.as_str())];
            parse(&snippet(&files, "main.cpp"), fingerprint).unwrap()
        })
        .collect();
    for (i, f) in alone.iter().enumerate() {
        assert!(f.contains(&format!("use_{i}")), "{f}");
        assert!(f.contains(&format!("undeclared_{i}")), "{f}");
    }
    std::thread::scope(|scope| {
        for i in 0..THREADS {
            let (texts, alone) = (&texts, &alone);
            scope.spawn(move || {
                let hname = format!("h{i}.h");
                let files =
                    [("main.cpp", texts[i].1.as_str()), (hname.as_str(), texts[i].0.as_str())];
                for round in 0..ROUNDS {
                    let got = parse(&snippet(&files, "main.cpp"), fingerprint).unwrap();
                    assert_eq!(got, alone[i], "thread {i}, round {round}");
                }
            });
        }
    });
}

// Nesting too deep for the stack is Kiln's fatal diagnostic: the unit test
// `snippet::tests::stack_guard` (it needs a smaller stack than `parse`
// gives, which only the crate can ask for).

/// `kiln <args> -x c++ -` on `text` from standard input: its standard
/// error without the summary line, the input named `main.cpp`.
fn kiln_stderr(args: &[&str], text: &str) -> String {
    let err = kiln_diag::SharedBuffer::new();
    let io = kiln_driver::Io {
        stdout: Box::new(std::io::sink()),
        stderr: kiln_driver::SharedWriter::new(err.clone()),
        stdin: Box::new(std::io::Cursor::new(text.as_bytes().to_vec())),
    };
    let mut line: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    line.extend(["-x", "c++", "-"].map(str::to_owned));
    kiln_driver::run_as("kiln", &line, io);
    err.contents()
        .replace("<stdin>", "main.cpp")
        .lines()
        .filter(|l| !l.ends_with(" generated."))
        .flat_map(|l| [l, "\n"])
        .collect()
}

// The language options are `kiln -std=<std> -fsyntax-only`'s: on sources
// that use no predefined macro and no header, the diagnostics are the
// same, text for text, in every standard.
#[test]
fn wp021_same_diagnostics_as_kiln() {
    let probes = [
        "int f() { return missing; }\n",
        "char8_t c;\n",
        "auto z = 1z;\n",
        "template <class... T> using F = T...[0];\n",
        "constexpr auto r = ^^int;\n",
        "int g() { int _ = 1; int _ = 2; }\n",
        "struct A { int operator[](int, int); };\n",
        "void h(int x) { auto y = auto(x); if (x = 1) {} }\n",
        "constexpr int k(int n) { return n ? n * k(n - 1) : 1; }\nstatic_assert(k(5) == 121);\n",
        "#if __has_builtin(__builtin_expect) && __has_feature(cxx_rtti)\nint yes;\n#else\n#error no\n#endif\n",
        "struct V { virtual ~V(); }; void t() { (void)typeid(V); }\n",
        "void u() { try { throw 1; } catch (int) {} }\n",
    ];
    for std in ["c++17", "c++20", "c++23", "c++26"] {
        let flag = format!("-std={std}");
        for text in probes {
            let files = [("main.cpp", text)];
            let s = Snippet { files: &files, main: "main.cpp", include: &[], std };
            let ours = parse(&s, |p| p.diagnostics.clone()).unwrap();
            let theirs = kiln_stderr(&[&flag, "-fsyntax-only"], text);
            assert_eq!(ours, theirs, "{std}: {text}");
            if text.contains("missing") || text.contains("121") || text.contains("typeid") {
                assert!(theirs.contains(": error: "), "{std}: {text}: {theirs}");
            }
            if text.contains("__has_builtin") || text.contains("try") {
                assert_eq!(theirs, "", "{std}: {text}");
            }
        }
    }
}
