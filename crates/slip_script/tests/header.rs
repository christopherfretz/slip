//! The resource headers (`spec/api.md` §2.1 rules 1 and 6, §2.2, §8.2;
//! work package WP-082): the embedded bytes, the resource directory, the
//! header check `transform-header`, and the headers under an ordinary C++26
//! compiler. Decisions: `spec/landed/api/WP-082.md`.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use slip_model::Digest;
use slip_model::hash::content_digest;
use slip_script::resource::{
    self, API_LEVEL, HEADERS, HeaderError, INTRINSICS_H, META_H, check_headers, resource_dir,
    write_to,
};

/// The repository's `resource/`.
fn repository_resource() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource")
}

/// A directory of its own under the system temp directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("slip-header-{}-{n}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Written to the test's standard error directly, not through `eprintln!`,
/// so that the test harness does not capture it: a skip and the compiler
/// used are visible in every run.
fn say(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

fn bytes_of(p: &Path) -> Vec<u8> {
    p.as_os_str().as_encoded_bytes().to_vec()
}

// ----- test 1: embedding (§2.1 rule 1, §2.2, §8.2) ---------------------------

// The embedded bytes are the files under `resource/include/slip/`, their
// digests are the content digests of those files, and the API level is the
// value of `__slip_lib_meta`.
#[test]
fn s2_2_embedded_bytes_are_the_repository_files() {
    let dir = repository_resource().join("include/slip");
    assert_eq!(META_H, std::fs::read(dir.join("meta.h")).unwrap().as_slice());
    assert_eq!(INTRINSICS_H, std::fs::read(dir.join("__intrinsics.h")).unwrap().as_slice());
    assert_eq!(HEADERS[0].bytes, META_H);
    assert_eq!(HEADERS[1].bytes, INTRINSICS_H);
    for h in &HEADERS {
        let file = std::fs::read(h.path_in(&repository_resource())).unwrap();
        assert_eq!(h.bytes, file.as_slice(), "{}", h.path);
        assert_eq!(h.digest(), content_digest(&file), "{}", h.path);
    }
    assert_eq!(resource::meta_h_digest(), content_digest(META_H));
    assert_eq!(resource::intrinsics_h_digest(), content_digest(INTRINSICS_H));
    assert_ne!(resource::meta_h_digest(), resource::intrinsics_h_digest());
}

// §8.2: `API_LEVEL` is the value `<slip/meta.h>` gives `__slip_lib_meta`,
// and every feature-test macro of the table is defined with its value.
#[test]
fn s8_2_api_level_is_slip_lib_meta() {
    let text = std::str::from_utf8(META_H).unwrap();
    let value = |name: &str| -> Option<String> {
        text.lines().find_map(|l| {
            let mut words = l.split_whitespace();
            (words.next() == Some("#define") && words.next() == Some(name))
                .then(|| words.next().unwrap_or_default().to_owned())
        })
    };
    assert_eq!(value("__slip_lib_meta"), Some(format!("{API_LEVEL}L")));
    for name in ["__slip_lib_meta_calls", "__slip_lib_meta_code", "__slip_lib_meta_edits"] {
        assert_eq!(value(name), Some("202611L".to_owned()), "{name}");
    }
}

// ----- the resource directory (§2.1 rule 1) -----------------------------------

// `$SLIP_RESOURCE_DIR` when set and not empty, as given; else the installed
// directory beside the executable when it exists; else the repository's.
#[test]
fn s2_1_resource_dir_order() {
    let s = Scratch::new("dir");
    let exe = s.0.join("bin/slip");
    let named = Path::new("relative/res");
    assert_eq!(resource_dir(&exe, Some(named.as_os_str())), named);
    // Not installed: the repository's `resource/` (a development build).
    let dev = resource_dir(&exe, None);
    assert_eq!(
        std::fs::canonicalize(&dev).unwrap(),
        std::fs::canonicalize(repository_resource()).unwrap()
    );
    assert_eq!(resource_dir(&exe, Some(OsStr::new(""))), dev, "an empty value is not set");
    // Installed.
    std::fs::create_dir_all(s.0.join("bin")).unwrap();
    std::fs::create_dir_all(s.0.join("share/slip/resource")).unwrap();
    assert_eq!(resource_dir(&exe, None), s.0.join("bin/../share/slip/resource"));
    assert_eq!(resource_dir(&exe, Some(named.as_os_str())), named, "the variable comes first");
}

// ----- `write_to` (§2.1 rule 1, `slip resource --write`) ---------------------

// The embedded files land at `include/slip/` under the directory, made if
// absent, replacing what was there, with no temporary file left; what was
// written passes the check.
#[test]
fn s2_1_write_to() {
    let s = Scratch::new("write");
    let dir = s.0.join("a/b");
    write_to(&dir).unwrap();
    std::fs::write(dir.join("include/slip/meta.h"), b"stale").unwrap();
    write_to(&dir).unwrap();
    let mut read = Vec::new();
    for h in &HEADERS {
        let p = h.path_in(&dir);
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(bytes, h.bytes, "{}", h.path);
        read.push((bytes_of(&p), content_digest(&bytes)));
    }
    let mut names: Vec<_> = std::fs::read_dir(dir.join("include/slip"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["__intrinsics.h", "meta.h"]);
    assert_eq!(check_headers(&read, &dir), Ok(()));
}

// An error names the path: a file where a directory must be made.
#[test]
fn s2_1_write_to_error_names_the_path() {
    let s = Scratch::new("write-error");
    std::fs::write(s.0.join("include"), b"a file").unwrap();
    let e = write_to(&s.0).unwrap_err();
    assert!(e.to_string().contains(&s.0.join("include").display().to_string()), "{e}");
}

// ----- test 2: the header check (§2.2, `transform-header`) -------------------

/// What a unit that included `<slip/meta.h>` from `resource` read: the main
/// file, the two headers, a library header.
fn unit_reading(resource: &Path) -> Vec<(Vec<u8>, Digest)> {
    vec![
        (b"/work/transform.cpp".to_vec(), content_digest(b"#include <slip/meta.h>\n")),
        (bytes_of(&HEADERS[0].path_in(resource)), content_digest(META_H)),
        (bytes_of(&HEADERS[1].path_in(resource)), content_digest(INTRINSICS_H)),
        (b"/usr/include/c++/14/string".to_vec(), content_digest(b"// string\n")),
    ]
}

#[test]
fn s2_2_check_accepts_this_builds_headers() {
    let res = Path::new("/opt/slip/share/slip/resource");
    assert_eq!(check_headers(&unit_reading(res), res), Ok(()));
    // Spelled with a trailing separator or a doubled one: the same path.
    let read = unit_reading(Path::new("/opt/slip/share/slip//resource/"));
    assert_eq!(check_headers(&read, res), Ok(()));
    // A relative resource directory, as given.
    let rel = Path::new("share/slip/resource");
    assert_eq!(check_headers(&unit_reading(rel), rel), Ok(()));
}

#[test]
fn s2_2_check_refuses_a_header_not_read() {
    let res = Path::new("/opt/slip/resource");
    for (i, name) in [(1, "slip/meta.h"), (2, "slip/__intrinsics.h")] {
        let mut read = unit_reading(res);
        read.remove(i);
        assert_eq!(check_headers(&read, res), Err(HeaderError::NotRead(name)));
    }
    assert_eq!(check_headers(&[], res), Err(HeaderError::NotRead("slip/meta.h")));
}

#[test]
fn s2_2_check_refuses_a_header_with_one_byte_changed() {
    let res = Path::new("/opt/slip/resource");
    for (i, h) in HEADERS.iter().enumerate() {
        let mut bytes = h.bytes.to_vec();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let mut read = unit_reading(res);
        read[i + 1].1 = content_digest(&bytes);
        assert_eq!(check_headers(&read, res), Err(HeaderError::Differs(h.name)));
    }
}

// The rule is about `<resource>`: the right bytes read from elsewhere (an
// `-I` of the author's that holds a copy) do not count.
#[test]
fn s2_2_check_refuses_the_right_header_from_another_directory() {
    let res = Path::new("/opt/slip/resource");
    let copy = Path::new("/home/author/vendored-slip");
    assert_eq!(check_headers(&unit_reading(copy), res), Err(HeaderError::NotRead("slip/meta.h")));
    // One of the two from elsewhere.
    let mut read = unit_reading(res);
    read[2].0 = bytes_of(&HEADERS[1].path_in(copy));
    assert_eq!(check_headers(&read, res), Err(HeaderError::NotRead("slip/__intrinsics.h")));
    // Not resolved: `..` that would lead to the same directory is another path.
    let around = Path::new("/opt/slip/x/../resource");
    assert_eq!(check_headers(&unit_reading(around), res), Err(HeaderError::NotRead("slip/meta.h")));
}

#[test]
fn s12_header_error_names_the_error() {
    assert_eq!(HeaderError::NAME, "transform-header");
    for e in [HeaderError::NotRead("slip/meta.h"), HeaderError::Differs("slip/__intrinsics.h")] {
        assert!(e.to_string().starts_with("transform-header: "), "{e}");
    }
}

// ----- test 3: the headers under another C++26 compiler (§2.1 rule 6) ---------

/// The compilers tried, in order. Each distinct one found on `PATH` that
/// accepts `-std=c++26` is used.
const COMPILERS: &[&str] = &[
    "g++",
    "g++-14",
    "g++-15",
    "g++-16",
    "clang++",
    "clang++-19",
    "clang++-20",
    "clang++-21",
    "clang++-22",
];

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

/// The distinct suitable compilers on `PATH`, by the file they resolve to.
fn suitable_compilers(scratch: &Path) -> Vec<(String, String)> {
    let probe = scratch.join("probe.cpp");
    std::fs::write(&probe, "int main() {}\n").unwrap();
    let mut seen = Vec::new();
    let mut found = Vec::new();
    for name in COMPILERS {
        let Some(path) = on_path(name) else { continue };
        let real = std::fs::canonicalize(&path).unwrap_or(path);
        if seen.contains(&real) {
            continue;
        }
        seen.push(real);
        let accepts = Command::new(name)
            .args(["-std=c++26", "-fsyntax-only"])
            .arg(&probe)
            .output()
            .is_ok_and(|o| o.status.success());
        let version = Command::new(name)
            .arg("--version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_owned())
            .unwrap_or_default();
        if accepts {
            found.push(((*name).to_owned(), version));
        } else {
            say(&format!("header.rs: {name} ({version}) does not accept -std=c++26; not used"));
        }
    }
    found
}

/// A warning line is allowed only if it is the one about `__call` being used
/// and never defined (GCC: "used but never defined"; Clang:
/// "-Wundefined-inline", "is not defined").
fn is_allowed_warning(line: &str) -> bool {
    line.contains("__call")
        && (line.contains("used but never defined") || line.contains("is not defined"))
}

#[test]
fn s2_1_6_examples_under_another_compiler() {
    let s = Scratch::new("cxx");
    let compilers = suitable_compilers(&s.0);
    if compilers.is_empty() {
        let reason = format!("no C++26 compiler among {COMPILERS:?} on PATH");
        if std::env::var_os("SLIP_REQUIRE_ALL").is_some_and(|v| v == "1") {
            panic!("SLIP_REQUIRE_ALL=1 and {reason}");
        }
        say(&format!("SKIP header.rs s2_1_6_examples_under_another_compiler: {reason}"));
        return;
    }
    let include = repository_resource().join("include");
    let examples = repository_resource().join("examples");
    for (cxx, version) in &compilers {
        for example in ["swap_arguments.cpp", "rename_function.cpp"] {
            let file = examples.join(example);
            let out = Command::new(cxx)
                .args(["-std=c++26", "-fsyntax-only", "-I"])
                .arg(&include)
                .arg(&file)
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&out.stderr);
            say(&format!(
                "header.rs: {cxx} ({version}) -std=c++26 -fsyntax-only {example}: {}\n{stderr}",
                out.status
            ));
            assert!(out.status.success(), "{cxx} {example}:\n{stderr}");
            for line in stderr.lines() {
                assert!(!line.contains("error:"), "{cxx} {example}: {line}");
                if line.contains("warning:") {
                    assert!(is_allowed_warning(line), "{cxx} {example}: {line}");
                }
            }
        }
    }
}
