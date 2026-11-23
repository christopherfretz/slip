//! The two examples of `spec/api.md` §10 are part of the conformance suite
//! (`docs/wp/WP-091.md`, "Tests"): the script test directories under
//! `tests/api/examples/swap/` and `tests/api/examples/rename/` run the
//! files `resource/examples/swap_arguments.cpp` and
//! `resource/examples/rename_function.cpp` themselves. Each directory's
//! `transform.cpp` is its header lines and one `#include` of the example,
//! nothing else, and the transform Kiln compiles read exactly the bytes of
//! the example file.

#![forbid(unsafe_code)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{loading, resource};
use slip_model::hash::content_digest;
use slip_script::script_test;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The one line after the header block of every `transform.cpp` of
/// `examples/<dir>/`: the include of `resource/examples/<file>`, found
/// from the host's include directory `resource/include` (`spec/api.md`
/// §2.1), outside the test's directory.
fn include_line(file: &str) -> String {
    format!("#include <../examples/{file}>")
}

/// The transform files of `examples/<dir>/`, checked to be the header
/// lines and the include of `file`, by their distinct bytes.
fn transforms(dir: &str, file: &str) -> Vec<PathBuf> {
    let root = crate_dir().join("tests/api/examples").join(dir);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut count = 0;
    for case in script_test::discover(&root).unwrap() {
        count += 1;
        let path = case.join(script_test::TRANSFORM);
        let text = std::fs::read_to_string(&path).unwrap();
        let body: Vec<&str> =
            text.lines().skip_while(|l| script_test::looks_like_header(l)).collect();
        assert_eq!(
            body,
            [include_line(file).as_str()],
            "{}: a transform of §10 is its header lines and the include of the example",
            path.display()
        );
        if seen.insert(text) {
            out.push(path);
        }
    }
    assert!(count > 1, "examples/{dir}/ has the subjects of WP-091's table");
    out
}

/// The canonical path of `p`, as Kiln recorded it (`ReadFile::path`).
fn canonical(p: &[u8]) -> Option<PathBuf> {
    std::fs::canonicalize(Path::new(std::str::from_utf8(p).ok()?)).ok()
}

/// §10: each distinct transform of `examples/<dir>/` loads, and the files
/// it read include `resource/examples/<file>` with that file's digest.
fn includes_the_example(dir: &str, file: &str) {
    let example = std::fs::canonicalize(resource().join("examples").join(file)).unwrap();
    let digest = content_digest(&std::fs::read(&example).unwrap());
    for transform in transforms(dir, file) {
        let read = loading(&resource(), &[], &transform, |l| match l {
            Ok(l) => l.files().to_vec(),
            Err(e) => panic!("{}: does not load: {e}", transform.display()),
        });
        let found = read.iter().find(|f| canonical(&f.path).as_deref() == Some(example.as_path()));
        let found = found.unwrap_or_else(|| {
            panic!("{}: the transform did not read {}", transform.display(), example.display())
        });
        assert_eq!(found.digest, digest, "{}: {file} read with other bytes", transform.display());
    }
}

/// §10.1: `examples/swap/` runs `resource/examples/swap_arguments.cpp`.
#[test]
fn s10_1_the_swap_directories_run_the_example() {
    includes_the_example("swap", "swap_arguments.cpp");
}

/// §10.2: `examples/rename/` runs `resource/examples/rename_function.cpp`.
#[test]
fn s10_2_the_rename_directories_run_the_example() {
    includes_the_example("rename", "rename_function.cpp");
}

/// WP-132 item 5: no example of `resource/examples/` writes text. Every
/// file there is run by some directory of `tests/api/examples/`, and every
/// such directory whose run completes is run again asking for the counter
/// `api.op.code_text` (`spec/api.md` §7.6 rule 6): it is absent, so `text`
/// (and `cat` of a string, which is `text`) never succeeded.
#[test]
fn wp132_no_example_writes_text() {
    let root = crate_dir().join("tests/api/examples");
    let mut covered = BTreeSet::new();
    let mut ran = 0;
    for case in script_test::discover(&root).unwrap() {
        let text = std::fs::read_to_string(case.join(script_test::TRANSFORM)).unwrap();
        let included = text
            .lines()
            .find_map(|l| l.strip_prefix("#include <../examples/")?.strip_suffix('>'))
            .unwrap_or_else(|| panic!("{}: includes no example", case.display()))
            .to_owned();
        let mut script = script_test::load_script(&case).unwrap();
        script.counters = vec!["api.op.code_text".to_owned()];
        let rendered = match script_test::run_script(&script, &resource()) {
            Ok(r) => r,
            Err(e) => panic!("{}: {e}", case.display()),
        };
        if rendered.starts_with("failed ") {
            continue;
        }
        ran += 1;
        assert!(
            rendered.contains("counter api.op.code_text absent\n"),
            "{}: the example {included} wrote text:\n{rendered}",
            case.display()
        );
        covered.insert(included);
    }
    assert!(ran > 0);
    let mut files = BTreeSet::new();
    for entry in std::fs::read_dir(resource().join("examples")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("cpp")) {
            files.insert(path.file_name().unwrap().to_str().unwrap().to_owned());
        }
    }
    assert_eq!(covered, files, "every example is run by a directory that completes");
}
