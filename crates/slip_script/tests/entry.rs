//! `spec/api.md` §2.1 to §2.3 and §12 through the driver
//! (`docs/wp/WP-089.md` "Tests"): each error of §12 from a transform a few
//! lines long, the order of the checks (WP-089 rule 1), and a transform
//! that loads. Decisions: `spec/landed/api/WP-089.md`.

mod common;

use common::{file, load, load_error, loading, resource, scratch, with_meta};
use slip_script::driver::LoadError;
use slip_script::resource::{HeaderError, write_to};

const OK: &str = "consteval void transform() {}";

/// The error of loading `OK` with the author's options `author`.
fn option_error(name: &str, author: &[&str]) -> LoadError {
    load(name, &with_meta(OK), author).expect_err("the options are refused")
}

// §12 `transform-option`, §2.1 rule 2: each refusal of
// `slip_kiln::session` passes on under the name of §12, with its text.
#[test]
fn s12_transform_option_each_refusal() {
    let cases: [(&[&str], &str); 8] = [
        (&["-O2"], "only -I, -isystem, -D, -U and -include may be given"),
        (&["-std=c++20"], "only -I, -isystem, -D, -U and -include may be given"),
        (&["-D", "@opts.rsp"], "response file"),
        (&["-I@x"], "response file"),
        (&["-DX=a\0b"], "NUL"),
        (&["-I", "relative/dir"], "not absolute"),
        (&["-DE=embed"], "`embed`"),
        (&["-include"], "missing"),
    ];
    for (i, (author, why)) in cases.into_iter().enumerate() {
        let e = option_error(&format!("opt{i}"), author);
        assert_eq!(e.name(), "transform-option", "{author:?}: {e}");
        let LoadError::Option(text) = &e else { panic!("{author:?}: {e:?}") };
        assert!(text.contains(why), "{author:?}: {text}");
    }
    // The transform file and the resource directory (§2.1 rule 2).
    let f = file("rel", &with_meta(OK));
    let relative = loading(std::path::Path::new("resource"), &[], &f, |l| l.map(|l| l.id()));
    assert_eq!(relative.unwrap_err().name(), "transform-option", "a relative resource directory");
    let relative_file =
        loading(&resource(), &[], std::path::Path::new("t.cpp"), |l| l.map(|l| l.id()));
    assert_eq!(relative_file.unwrap_err().name(), "transform-option", "a relative transform file");
    let at_file = loading(&resource(), &[], std::path::Path::new("@t.cpp"), |l| l.map(|l| l.id()));
    assert_eq!(
        at_file.unwrap_err().name(),
        "transform-option",
        "a transform file beginning with @"
    );
}

// §12 `transform-compile`, §2.1 rule 5: an error in the unit, with Kiln's
// rendering.
#[test]
fn s12_transform_compile_an_error() {
    let e =
        load("compile", &with_meta("consteval void transform() { int x = ; }"), &[]).unwrap_err();
    assert_eq!(e.name(), "transform-compile");
    let LoadError::Compile { rendered } = &e else { panic!("{e:?}") };
    assert!(rendered.contains("compile.cpp:") && rendered.contains("error:"), "{rendered}");
}

// §12 `transform-compile`, §2.1 rule 6: `#embed` is refused naming the
// file and the line.
#[test]
fn s12_transform_compile_embed() {
    let e = load(
        "embed",
        &with_meta("#if 0\n#embed \"x\"\n#endif\nconsteval void transform() {}"),
        &[],
    )
    .unwrap_err();
    assert_eq!(e.name(), "transform-compile");
    let LoadError::Compile { rendered } = &e else { panic!("{e:?}") };
    assert!(rendered.contains("embed.cpp:3:"), "the file and the line: {rendered}");
    assert!(rendered.contains("`#embed` is not supported in a transform"), "{rendered}");
}

// §12 `transform-compile`, decision 2 of the landed file: a transform file
// that cannot be read is Kiln stopping before the unit; the author gets
// Kiln's text under `transform-compile`.
#[test]
fn s12_transform_compile_no_such_file() {
    let missing = scratch("missing").join("missing.cpp");
    let e = loading(&resource(), &[], &missing, |l| l.map(|l| l.id())).unwrap_err();
    assert_eq!(e.name(), "transform-compile", "{e}");
    let LoadError::Compile { rendered } = &e else { panic!("{e:?}") };
    assert!(rendered.contains("missing.cpp"), "{rendered}");
}

// §12 `transform-header`, §2.2: a unit that does not read `<slip/meta.h>`,
// and one that reads a `<slip/meta.h>` with other bytes.
#[test]
fn s12_transform_header() {
    assert_eq!(
        load("noheader", OK, &[]).unwrap_err(),
        LoadError::Header(HeaderError::NotRead("slip/meta.h"))
    );
    let other = scratch("res");
    write_to(&other).unwrap();
    let meta = other.join("include/slip/meta.h");
    let mut bytes = std::fs::read(&meta).unwrap();
    bytes.extend_from_slice(b"\n// one more line\n");
    std::fs::write(&meta, bytes).unwrap();
    let f = file("header", &with_meta(OK));
    let e = loading(&other, &[], &f, |l| l.map(|l| l.id())).unwrap_err();
    assert_eq!(e, LoadError::Header(HeaderError::Differs("slip/meta.h")));
    assert_eq!(e.name(), "transform-header");
}

// §12 and §2.3 rule 2: each entry error, in a transform a few lines long.
#[test]
fn s12_entry_errors() {
    let cases = [
        ("missing", "consteval void other() {}", "entry-missing"),
        (
            "using",
            "namespace n { consteval void transform() {} } using n::transform;",
            "entry-missing",
        ),
        (
            "overloaded",
            "consteval void transform() {} consteval void transform(int) {}",
            "entry-overloaded",
        ),
        ("notfunction", "int transform = 0;", "entry-not-function"),
        ("template", "template <class T> consteval void transform() {}", "entry-not-function"),
        ("notconsteval", "constexpr void transform() {}", "entry-not-consteval"),
        ("parameters", "consteval void transform(int) {}", "entry-parameters"),
        ("type", "consteval int transform() { return 0; }", "entry-type"),
        ("undefined", "consteval void transform();", "entry-undefined"),
        ("deleted", "consteval void transform() = delete;", "entry-undefined"),
        ("reduce", "consteval void transform() {} consteval void reduce() {}", "entry-reserved"),
        ("prepare", "consteval void transform() {} void prepare();", "entry-reserved"),
        (
            "repair",
            "consteval void transform() {} namespace { template <class T> void repair(T); }",
            "entry-reserved",
        ),
    ];
    for (name, body, error) in cases {
        assert_eq!(load_error(name, &with_meta(body)), error, "{name}");
    }
    // §2.3 rule 4: a reserved name brought in by a using-directive is not
    // declared by the unit.
    let used = with_meta(
        "namespace n { void reduce(); } using namespace n; consteval void transform() {}",
    );
    assert!(load("usingreduce", &used, &[]).is_ok());
    // §2.3 rule 2 (L3 of the review): the name designates one function, so
    // a declaration followed by its definition loads.
    let redeclared = with_meta("consteval void transform();\nconsteval void transform() {}");
    let id = load("redeclared", &redeclared, &[]);
    assert!(id.is_ok(), "{id:?}");
}

// WP-089 rule 1: the first check that fails is the error. Each transform
// has two errors and gets the first: options before the compile, the
// compile before the headers, the headers before the reserved names, the
// reserved names before the entry.
#[test]
fn rule1_the_first_error_wins() {
    let bad_compile = "consteval void transform() { int x = ; }";
    assert_eq!(
        load("o_c", &with_meta(bad_compile), &["-O2"]).unwrap_err().name(),
        "transform-option"
    );
    assert_eq!(load("c_h", bad_compile, &[]).unwrap_err().name(), "transform-compile");
    assert_eq!(
        load("h_r", "consteval void reduce() {}", &[]).unwrap_err().name(),
        "transform-header"
    );
    assert_eq!(
        load_error("r_e", &with_meta("consteval void reduce() {} int transform;")),
        "entry-reserved"
    );
}

// A transform that loads, with every option kind (§2.1 rule 2).
#[test]
fn s2_1_a_transform_that_loads() {
    let dir = scratch("inc");
    std::fs::write(dir.join("part.h"), "constexpr int part = 1;\n").unwrap();
    std::fs::write(dir.join("pre.h"), "constexpr int pre = 2;\n").unwrap();
    let inc = dir.to_str().unwrap();
    let pre = dir.join("pre.h");
    let author = ["-I", inc, "-isystem", inc, "-DN=3", "-UM", "-include", pre.to_str().unwrap()];
    let id = load(
        "loads",
        &with_meta(
            "#include \"part.h\"\nconsteval void transform() { static_assert(part + pre + N == 6); }",
        ),
        &author,
    );
    assert!(id.is_ok(), "{id:?}");
}
