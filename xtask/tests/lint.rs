//! `cargo xtask lint` (spec/harness.md §3). The fixture of `common` passes
//! every lint; each test below breaks one lint and checks the exact
//! violation lines.

mod common;

use common::Fixture;
use std::process::Command;

/// The violations of the fixture, rendered.
fn lint(f: &Fixture) -> Vec<String> {
    xtask::lint::run(f.root()).unwrap().iter().map(ToString::to_string).collect()
}

fn none() -> Vec<String> {
    Vec::new()
}

// §3: the passing fixture passes every lint.
#[test]
fn s3_fixture_passes_every_lint() {
    let f = Fixture::new("pass");
    assert_eq!(lint(&f), none());
}

// §3: the output format `<path>[:<line>]: <lint>: <message>`, lints in the
// order of the table, and the binary's exit status.
#[test]
fn s3_binary_reports_and_exits_1() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask")).args(["lint", "extra"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2), "a usage error");
    let v = xtask::lint::Violation::new("a/b.md", Some(3), "doc-links", "m");
    assert_eq!(v.to_string(), "a/b.md:3: doc-links: m");
    let v = xtask::lint::Violation::new("a/b.md", None, "spec-shape", "m");
    assert_eq!(v.to_string(), "a/b.md: spec-shape: m");
}

#[test]
fn s3_violations_follow_the_table_order() {
    let f = Fixture::new("order");
    f.write("docs/x.md", "[a](missing.md)\n");
    f.write("crates/slip_cdb/src/lib.rs", "// no attribute\n");
    assert_eq!(
        lint(&f),
        [
            "crates/slip_cdb/src/lib.rs: forbid-unsafe: crate root does not contain \
             `#![forbid(unsafe_code)]`",
            "docs/x.md:1: doc-links: link `missing.md` points at no file",
        ]
    );
}

// §3 `kiln-boundary`, first rule: only crates/slip_kiln/Cargo.toml names a
// `kiln_*` crate, under any dependency table and through a rename.
#[test]
fn s3_kiln_boundary_kiln_crates() {
    let f = Fixture::new("kb1");
    f.write(
        "crates/slip_model/Cargo.toml",
        "[package]\nname = \"slip_model\"\n\n[dependencies]\nkiln_ast.workspace = true\n\n\
         [target.'cfg(unix)'.dev-dependencies]\nlex = { path = \"../x\", package = \"kiln_lex\" }\n\
         \n[build-dependencies.kiln_pp]\npath = \"../y\"\n",
    );
    assert_eq!(
        lint(&f),
        [
            "crates/slip_model/Cargo.toml:5: kiln-boundary: names Kiln crate `kiln_ast`; only \
             crates/slip_kiln/Cargo.toml may",
            "crates/slip_model/Cargo.toml:8: kiln-boundary: names Kiln crate `kiln_lex`; only \
             crates/slip_kiln/Cargo.toml may",
            "crates/slip_model/Cargo.toml:10: kiln-boundary: names Kiln crate `kiln_pp`; only \
             crates/slip_kiln/Cargo.toml may",
        ]
    );
}

// §3 `kiln-boundary`, second rule: dependents of slip_kiln, directly or
// indirectly, are only the four crates that link Kiln.
#[test]
fn s3_kiln_boundary_dependents() {
    let f = Fixture::new("kb2");
    // Direct.
    f.write(
        "crates/slip_cdb/Cargo.toml",
        "[package]\nname = \"slip_cdb\"\n\n[dependencies]\nslip_kiln.workspace = true\n",
    );
    // Indirect, through slip_digest, as a dev-dependency.
    f.write(
        "xtask/Cargo.toml",
        "[package]\nname = \"xtask\"\n\n[dev-dependencies]\nslip_digest = { path = \"x\" }\n",
    );
    assert_eq!(
        lint(&f),
        [
            "crates/slip_cdb/Cargo.toml:5: kiln-boundary: `slip_cdb` depends on `slip_kiln` \
             (slip_cdb -> slip_kiln); only slip_digest, slip_script and slip_cli may",
            "xtask/Cargo.toml:5: kiln-boundary: `xtask` depends on `slip_kiln` \
             (xtask -> slip_digest -> slip_kiln); only slip_digest, slip_script and slip_cli may",
        ]
    );
}

// §3 `kiln-boundary`, third rule (spec/landed/harness/WP-021.md): the
// paths `slip_kiln::kiln` and `slip_kiln::snippet`, in the path form and
// in a `use` group, outside the files that may name them; comments and
// literals are not code.
#[test]
fn s3_kiln_boundary_kiln_modules() {
    let f = Fixture::new("kb4");
    // Allowed: slip_digest and slip_script name both; slip_kiln itself.
    f.write(
        "crates/slip_digest/src/walk.rs",
        "use slip_kiln::kiln::ast::Ast;\nuse slip_kiln::{kiln, snippet};\n",
    );
    f.write(
        "crates/slip_script/src/lib.rs",
        "#![forbid(unsafe_code)]\nuse slip_kiln::snippet::parse;\n",
    );
    f.write(
        "crates/slip_kiln/src/x.rs",
        "use crate::kiln;\nfn f() { slip_kiln::kiln::ast::dump(); }\n",
    );
    // `snippet` in the tests of a crate that links Kiln.
    f.write("crates/slip_cli/tests/walk.rs", "use slip_kiln::snippet::{Snippet, parse};\n");
    // Refused: both paths in slip_cli/src, the path form and the group
    // form (one item per line), and `kiln` in slip_cli's tests.
    f.write(
        "crates/slip_cli/src/a.rs",
        "fn f() {\n    let _ = slip_kiln :: kiln::ast::Ast::new;\n}\nuse slip_kiln::snippet;\n\
         use ::slip_kiln::{\n    KilnCheck,\n    snippet::parse,\n    kiln as k,\n    x::{kiln},\n};\n",
    );
    f.write("crates/slip_cli/tests/t.rs", "use slip_kiln::kiln::ast;\n");
    // Ignored: in comments and literals, and other paths.
    f.write(
        "crates/slip_cli/src/b.rs",
        "// slip_kiln::kiln\n/* use slip_kiln::{snippet}; */\n/// slip_kiln::snippet::parse\n\
         const A: &str = \"slip_kiln::kiln\";\nconst B: &str = r#\"use slip_kiln::{kiln}\"#;\n\
         use slip_kiln::kiln_version;\nfn g<'a>(x: &'a str) -> char { 'k' }\n",
    );
    assert_eq!(
        lint(&f),
        [
            "crates/slip_cli/src/a.rs:2: kiln-boundary: names `slip_kiln::kiln`; only \
             crates/slip_digest/ and crates/slip_script/ may",
            "crates/slip_cli/src/a.rs:4: kiln-boundary: names `slip_kiln::snippet`; only \
             crates/slip_digest/, crates/slip_script/ and the tests/ of slip_kiln, slip_digest, \
             slip_script and slip_cli may",
            "crates/slip_cli/src/a.rs:7: kiln-boundary: names `slip_kiln::snippet`; only \
             crates/slip_digest/, crates/slip_script/ and the tests/ of slip_kiln, slip_digest, \
             slip_script and slip_cli may",
            "crates/slip_cli/src/a.rs:8: kiln-boundary: names `slip_kiln::kiln`; only \
             crates/slip_digest/ and crates/slip_script/ may",
            "crates/slip_cli/tests/t.rs:1: kiln-boundary: names `slip_kiln::kiln`; only \
             crates/slip_digest/ and crates/slip_script/ may",
        ]
    );
}

// §3 `kiln-boundary`: a directory outside `members` (or in `exclude`) is
// not a member and not checked.
#[test]
fn s3_kiln_boundary_only_members() {
    let f = Fixture::new("kb3");
    let bad = "[package]\nname = \"other\"\n\n[dependencies]\nkiln_ast = \"1\"\n";
    f.write("kiln/crates/kiln_x/Cargo.toml", bad);
    f.write("tools/other/Cargo.toml", bad);
    assert_eq!(lint(&f), none());
}

// §3 `forbid-unsafe`: lib.rs, main.rs and every file under src/bin/.
#[test]
fn s3_forbid_unsafe() {
    let f = Fixture::new("fu");
    f.write("crates/slip_model/src/bin/two/main.rs", "fn main() {}\n");
    f.write("crates/slip_model/src/lib.rs", "// #![forbid(unsafe_code)] in a comment\n");
    f.write("xtask/src/main.rs", "#![deny(unsafe_code)]\n");
    assert_eq!(
        lint(&f),
        [
            "crates/slip_model/src/bin/two/main.rs: forbid-unsafe: crate root does not contain \
             `#![forbid(unsafe_code)]`",
            "crates/slip_model/src/lib.rs: forbid-unsafe: crate root does not contain \
             `#![forbid(unsafe_code)]`",
            "xtask/src/main.rs: forbid-unsafe: crate root does not contain \
             `#![forbid(unsafe_code)]`",
        ]
    );
}

// §3 `dependencies` and §3.1: a third-party crate neither in the allow
// list nor in Kiln's lock at the same version.
#[test]
fn s3_dependencies() {
    let f = Fixture::new("dep");
    let lock = common::BASE.iter().find(|(p, _)| *p == "Cargo.lock").unwrap().1;
    f.write(
        "Cargo.lock",
        &format!(
            "{lock}\n[[package]]\nname = \"regex\"\nversion = \"1.0.0\"\n\
             source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n\
             [[package]]\nname = \"hashbrown\"\nversion = \"0.14.0\"\n\
             source = \"git+https://example.org/hashbrown\"\n\n\
             [[package]]\nname = \"libc\"\nversion = \"9.9.9\"\n\
             source = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
        ),
    );
    assert_eq!(
        lint(&f),
        [
            "Cargo.lock:18: dependencies: third-party crate `regex 1.0.0` is not allowed by \
             spec/harness.md §3.1",
            "Cargo.lock:23: dependencies: third-party crate `hashbrown 0.14.0` is not allowed by \
             spec/harness.md §3.1",
        ]
    );
}

// §3.1: Kiln's crates are allowed only as Kiln's lock lists them.
#[test]
fn s3_dependencies_without_kiln_lock() {
    let f = Fixture::new("dep2");
    f.remove("kiln/Cargo.lock");
    assert_eq!(
        lint(&f),
        ["Cargo.lock:4: dependencies: third-party crate `hashbrown 0.15.5` is not allowed by \
             spec/harness.md §3.1 (kiln/Cargo.lock cannot be read)"]
    );
}

// §3 `spec-index`: a spec without a row, a row naming a missing file, a
// bad status.
#[test]
fn s3_spec_index() {
    let f = Fixture::new("si");
    f.write("spec/cdb.md", "# cdb\n\n## Tests\n\n## Open questions\n");
    f.write(
        "spec/README.md",
        "# Specs\n\n## Index\n\n| Spec | Status |\n|---|---|\n| [model.md](model.md) | draft |\n\
         | [exec.md](exec.md) | stable |\n| `verify.md` | written |\n| digest.md | not written |\n",
    );
    assert_eq!(
        lint(&f),
        [
            "spec/README.md:8: spec-index: row `exec.md` has status `stable` but spec/exec.md \
             does not exist",
            "spec/README.md:9: spec-index: status `written` is not `not written`, `draft` or \
             `stable`",
            "spec/README.md:9: spec-index: row `verify.md` has status `written` but \
             spec/verify.md does not exist",
            "spec/cdb.md: spec-index: has no row in spec/README.md's index",
            "spec/README.md:8: doc-links: link `exec.md` points at no file",
        ]
    );
}

#[test]
fn s3_spec_index_missing_table() {
    let f = Fixture::new("si2");
    f.write("spec/README.md", "# Specs\n\n```\n## Index\n```\n");
    assert_eq!(lint(&f), ["spec/README.md: spec-index: no `## Index` heading"]);
}

// §3 `spec-shape`: both headings, as lines of their own outside code.
#[test]
fn s3_spec_shape() {
    let f = Fixture::new("ss");
    f.write("spec/model.md", "# model\n\n### Tests\n\n```\n## Open questions\n```\n");
    assert_eq!(
        lint(&f),
        [
            "spec/model.md: spec-shape: missing the heading `## Tests`",
            "spec/model.md: spec-shape: missing the heading `## Open questions`",
        ]
    );
}

// §3 `landed`: the path shape, an existing spec, the title line, an
// existing work package.
#[test]
fn s3_landed() {
    let f = Fixture::new("la");
    f.write("spec/landed/model.md", "# WP-001 — x\n");
    f.write("spec/landed/model/notes.md", "# WP-001 — x\n");
    f.write("spec/landed/nosuch/WP-001.md", "# WP-001 — x\n");
    f.write("spec/landed/model/WP-002.md", "# WP-001 — wrong id\n");
    f.write("spec/landed/model/WP-007.md", "# WP-007 — no work package\n");
    assert_eq!(
        lint(&f),
        [
            "spec/landed/model.md: landed: is not `spec/landed/<spec>/WP-NNN.md`",
            "spec/landed/model/WP-002.md:1: landed: does not start with `# WP-002 — `",
            "spec/landed/model/WP-007.md: landed: docs/wp/WP-007.md does not exist",
            "spec/landed/model/notes.md: landed: is not `spec/landed/<spec>/WP-NNN.md`",
            "spec/landed/nosuch/WP-001.md: landed: names spec `nosuch`, but spec/nosuch.md does \
             not exist",
        ]
    );
}

// §3 `wp`: title, the five lines and their values, `Needs` naming files.
#[test]
fn s3_wp() {
    let f = Fixture::new("wp");
    f.write(
        "docs/wp/WP-003.md",
        "# WP-004 — wrong id\n\n- Status: started\n- Milestone: 0\n- Track: Z\n\
         - Needs: WP-001, WP-099, later\n- Size: XL\n",
    );
    f.write("docs/wp/WP-004.md", "# WP-004 — short\n- Status: todo\n");
    f.write("docs/wp/WP-x.md", "# WP-x — bad name\n");
    assert_eq!(
        lint(&f),
        [
            "docs/wp/WP-003.md:1: wp: the first line is not `# WP-003 — <title>`",
            "docs/wp/WP-003.md:3: wp: `Status` is `started`, not one of todo, in-progress, \
             review, done, dropped",
            "docs/wp/WP-003.md:4: wp: `Milestone` is `0`, not `M` and a number",
            "docs/wp/WP-003.md:5: wp: `Track` is `Z`, not one of A to H",
            "docs/wp/WP-003.md:6: wp: `Needs` names WP-099, which has no file in docs/wp/",
            "docs/wp/WP-003.md:6: wp: `Needs` entry `later` is not a work-package id or `none`",
            "docs/wp/WP-003.md:7: wp: `Size` is `XL`, not one of S, M, L",
            "docs/wp/WP-004.md:2: wp: the second line is not blank",
            "docs/wp/WP-004.md:3: wp: line 3 is not `- Status: <value>`",
            "docs/wp/WP-004.md:4: wp: line 4 is not `- Milestone: <value>`",
            "docs/wp/WP-004.md:5: wp: line 5 is not `- Track: <value>`",
            "docs/wp/WP-004.md:6: wp: line 6 is not `- Needs: <value>`",
            "docs/wp/WP-004.md:7: wp: line 7 is not `- Size: <value>`",
            "docs/wp/WP-x.md: wp: file name is not `WP-NNN.md`",
        ]
    );
}

// §3 `local-paths`. The paths are assembled at run time so this file
// spells none (spec/harness.md §3.2).
#[test]
fn s3_local_paths() {
    let f = Fixture::new("lp");
    let home = concat!("/home/", "alice/src");
    let vol = concat!("/Volumes/", "W/x/kiln++");
    let users = concat!("/Users/", "c/lib");
    let root = concat!("/root/", ".cargo/bin");
    f.write(
        "scripts/run.sh",
        &format!(
            "cd {home} && make\n\"compiler\": \"{vol}\",\ncc -I{home}/inc -L{users}\n`{root}`\n"
        ),
    );
    // Binary files are skipped.
    std::fs::write(f.path("data.bin"), format!("\0{home}")).unwrap();
    assert_eq!(
        lint(&f),
        [
            format!("scripts/run.sh:1: local-paths: absolute local path `{home}`"),
            format!("scripts/run.sh:2: local-paths: absolute local path `{vol}`"),
            format!("scripts/run.sh:3: local-paths: absolute local path `{home}/inc`"),
            format!("scripts/run.sh:3: local-paths: absolute local path `{users}`"),
            format!("scripts/run.sh:4: local-paths: absolute local path `{root}`"),
        ]
    );
}

#[test]
fn s3_local_paths_are_not_prefixes_inside_paths() {
    for line in [
        "`/home/`, `/Users/`, `/root/`, `/Volumes/` (bare prefixes)",
        "a/root/b <sysroot>/root/x $(pwd)/root/y ${HOME}/x",
        "/usr/bin/clang /tmp/x /mnt/work target/release/slip",
    ] {
        assert!(xtask::lint::local_paths(line).is_empty(), "{line}");
    }
}

// §3 `doc-links`: relative links in docs/, spec/ and README.md; a link
// outside the root, to a missing file, a reference definition; links in
// other directories are not checked.
#[test]
fn s3_doc_links() {
    let f = Fixture::new("dl");
    f.write(
        "spec/model.md",
        "# model\n\n[gone](gone/a.md#x) [up](../../outside.md) [ok](../README.md)\n\n\
         [def]: ./nope/\n\n## Tests\n\n## Open questions\n",
    );
    f.write("other/x.md", "[gone](gone.md)\n");
    f.write("README.md", "# Slip\n\n![logo](assets/logo%20big.svg)\n");
    assert_eq!(
        lint(&f),
        [
            "README.md:3: doc-links: link `assets/logo%20big.svg` points at no file",
            "spec/model.md:3: doc-links: link `gone/a.md#x` points at no file",
            "spec/model.md:3: doc-links: link `../../outside.md` points at no file",
            "spec/model.md:5: doc-links: link `./nope/` points at no file",
        ]
    );
    f.write("assets/logo big.svg", "<svg/>");
    f.write("spec/nope/a.md", "");
    f.write("spec/gone/a.md", "");
    f.write("spec/model.md", "# model\n\n[gone](gone/a.md#x)\n\n## Tests\n\n## Open questions\n");
    assert_eq!(lint(&f), none());
}

fn git(root: &std::path::Path, args: &[&str]) {
    let st = Command::new("git").args(args).current_dir(root).output().unwrap();
    assert!(st.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&st.stderr));
}

// §3: in a git checkout the lints read `git ls-files`: an untracked file
// is not read, and a tracked one is.
#[test]
fn s3_reads_tracked_files_in_a_checkout() {
    let f = Fixture::new("git");
    git(f.root(), &["init", "-q"]);
    git(f.root(), &["add", "-A"]);
    let home = concat!("/home/", "bob/x");
    f.write("untracked.txt", home);
    f.write("crates/slip_model/src/bin/untracked.rs", "fn main() {}\n");
    assert_eq!(lint(&f), none());
    f.write("tracked.txt", home);
    git(f.root(), &["add", "tracked.txt"]);
    assert_eq!(lint(&f), [format!("tracked.txt:1: local-paths: absolute local path `{home}`")]);
}

// WP-090 `api-tests` (spec/landed/api/WP-090.md): every script test
// directory under crates/slip_script/tests/api/ (one with a transform.cpp)
// has t.cpp and expect.txt; every file there is in one; none is inside
// another; the `// spec:` header line names a heading of spec/api.md.
#[test]
fn wp090_api_tests() {
    let f = Fixture::new("api");
    let api = "crates/slip_script/tests/api";
    f.write("spec/api.md", "# api\n\n## 2. U\n\n### 2.1 Unit\n\n## Tests\n\n## Open questions\n");
    f.write(
        "spec/README.md",
        &common::BASE
            .iter()
            .find(|(p, _)| *p == "spec/README.md")
            .unwrap()
            .1
            .replace("| digest.md |", "| [api.md](api.md) | The API | draft |\n| digest.md |"),
    );
    let good = |dir: &str, spec: &str| {
        f.write(&format!("{api}/{dir}/transform.cpp"), &format!("// args: -DX\n{spec}\nint x;\n"));
        f.write(&format!("{api}/{dir}/t.cpp"), "int y;\n");
        f.write(&format!("{api}/{dir}/expect.txt"), "transform <id>\n");
    };
    good("format/a", "// spec: §2.1 rule 3");
    good("format/b", "// spec: §2");
    f.write(&format!("{api}/format/b/inc/lib.h"), "int z;\n");
    assert_eq!(lint(&f), none());

    good("format/no-heading", "// spec: §2.2");
    good("format/no-form", "// spec: 2.1");
    good("format/no-spec", "// xfail: why");
    f.write(&format!("{api}/format/after/transform.cpp"), "int x;\n// spec: §2\n");
    f.write(&format!("{api}/format/after/t.cpp"), "");
    f.write(&format!("{api}/format/after/expect.txt"), "");
    f.write(&format!("{api}/format/missing/transform.cpp"), "// spec: §2\n");
    good("format/a/nested", "// spec: §2");
    f.write(&format!("{api}/format/stray.txt"), "");
    f.write(&format!("{api}/values/t.cpp"), "");
    assert_eq!(
        lint(&f),
        [
            format!(
                "{api}/format/a/nested/transform.cpp: api-tests: a script test inside the directory of the script test {api}/format/a"
            ),
            format!("{api}/format/after/transform.cpp: api-tests: has no `// spec:` header line"),
            format!("{api}/format/missing: api-tests: has no t.cpp"),
            format!("{api}/format/missing: api-tests: has no expect.txt"),
            format!(
                "{api}/format/no-form/transform.cpp:2: api-tests: `// spec: 2.1` does not name a section as `§N` or `§N.M`"
            ),
            format!(
                "{api}/format/no-heading/transform.cpp:2: api-tests: `// spec: §2.2`: spec/api.md has no heading §2.2"
            ),
            format!("{api}/format/no-spec/transform.cpp: api-tests: has no `// spec:` header line"),
            format!(
                "{api}/format/stray.txt: api-tests: is in no script test directory (one with a transform.cpp)"
            ),
            format!(
                "{api}/values/t.cpp: api-tests: is in no script test directory (one with a transform.cpp)"
            ),
        ]
    );
    f.remove("spec/api.md");
    let unread: Vec<String> =
        lint(&f).into_iter().filter(|l| l.contains("cannot be read")).collect();
    assert_eq!(
        unread,
        [
            format!(
                "{api}/format/a/nested/transform.cpp:2: api-tests: `// spec: §2`: spec/api.md cannot be read"
            ),
            format!(
                "{api}/format/a/transform.cpp:2: api-tests: `// spec: §2.1 rule 3`: spec/api.md cannot be read"
            ),
            format!(
                "{api}/format/b/transform.cpp:2: api-tests: `// spec: §2`: spec/api.md cannot be read"
            ),
            format!(
                "{api}/format/missing/transform.cpp:1: api-tests: `// spec: §2`: spec/api.md cannot be read"
            ),
            format!(
                "{api}/format/no-heading/transform.cpp:2: api-tests: `// spec: §2.2`: spec/api.md cannot be read"
            ),
        ]
    );
}
