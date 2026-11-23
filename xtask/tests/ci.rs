//! `cargo xtask ci` (spec/harness.md §2), with a fake `cargo`: a shell
//! script that logs its arguments and fails when they contain the word in
//! `FAKE_CARGO_FAIL`. Neither the real `kiln` step nor the full gate runs
//! here.

#![cfg(unix)]

mod common;

use common::Fixture;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use xtask::ci::{self, Options};

const FAKE: &str = "#!/bin/sh\n\
    [ \"$1\" = --probe ] && exit 0\n\
    echo \"$*\" >> \"$FAKE_CARGO_LOG\"\n\
    case \" $* \" in *\" $FAKE_CARGO_FAIL \"*) exit 1 ;; esac\n\
    exit 0\n";

/// A fixture holding the fake cargo at `bin/cargo` and its log at `log`.
fn fake(tag: &str) -> (Fixture, PathBuf, PathBuf) {
    let f = Fixture::new(tag);
    f.write("bin/cargo", FAKE);
    let cargo = f.path("bin/cargo");
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    common::wait_executable(&cargo);
    let log = f.path("log");
    (f, cargo, log)
}

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

/// Runs the xtask binary's `ci` with the fake cargo; (exit code, stdout,
/// the fake's log).
fn run_binary(a: &[&str], fail: &str, tag: &str) -> (Option<i32>, String, String) {
    let (_f, cargo, log) = fake(tag);
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("ci")
        .args(a)
        .env("CARGO", &cargo)
        .env("FAKE_CARGO_LOG", &log)
        .env("FAKE_CARGO_FAIL", fail)
        .output()
        .unwrap();
    let log = std::fs::read_to_string(&log).unwrap_or_default();
    (out.status.code(), String::from_utf8(out.stdout).unwrap(), log)
}

/// The output lines with the seconds of `ok (…s)` replaced by `T`, after
/// checking they have one decimal.
fn normalise(out: &str) -> Vec<String> {
    out.lines()
        .map(|l| match l.split_once(": ok (") {
            Some((name, secs)) => {
                let secs = secs.strip_suffix("s)").unwrap_or_else(|| panic!("{l}"));
                let (whole, frac) = secs.split_once('.').unwrap_or_else(|| panic!("{l}"));
                assert!(whole.parse::<u64>().is_ok() && frac.len() == 1, "{l}");
                format!("{name}: ok (Ts)")
            }
            None => l.to_owned(),
        })
        .collect()
}

// §2: step selection.
#[test]
fn s2_parse_selects_steps_in_gate_order() {
    let steps = |a: &[&str]| ci::parse_args(&args(a)).map(|o| o.steps);
    let all = vec!["lint", "fmt", "clippy", "test", "kiln", "transform"];
    assert_eq!(steps(&[]), Ok(all));
    assert_eq!(steps(&["--quick"]), Ok(vec!["lint", "fmt", "clippy", "test"]));
    assert_eq!(
        steps(&["--step", "test", "--step", "lint", "--step", "test"]),
        Ok(vec!["lint", "test"])
    );
    assert_eq!(steps(&["--step", "kiln"]), Ok(vec!["kiln"]));
    assert_eq!(steps(&["--quick", "--step", "fmt"]), Ok(vec!["fmt"]));
    assert_eq!(ci::parse_args(&args(&["--keep-going"])).map(|o| o.keep_going), Ok(true));
}

// §2: usage errors exit 2 and run nothing.
#[test]
fn s2_usage_errors() {
    for a in [&["--step"][..], &["--step", "nosuch"], &["--quick", "--step", "kiln"], &["--fast"]] {
        assert!(ci::parse_args(&args(a)).is_err(), "{a:?}");
        let (code, out, log) = run_binary(a, "", "usage");
        assert_eq!((code, out.as_str(), log.as_str()), (Some(2), "", ""), "{a:?}");
    }
}

// §2: the commands each step runs, through `$CARGO`, and the output lines.
#[test]
fn s2_binary_runs_cargo_steps() {
    let (code, out, log) =
        run_binary(&["--step", "test", "--step", "clippy", "--step", "fmt"], "", "bin");
    assert_eq!(code, Some(0));
    assert_eq!(
        normalise(&out),
        [
            "==> fmt",
            "fmt: ok (Ts)",
            "==> clippy",
            "clippy: ok (Ts)",
            "==> test",
            "test: ok (Ts)",
            "ci: ok"
        ]
    );
    assert_eq!(log, "fmt --all --check\nclippy --all-targets -- -D warnings\ntest\n");
}

// §2: a failing step stops the gate and the exit status is 1.
#[test]
fn s2_failure_stops_the_gate() {
    let (code, out, log) =
        run_binary(&["--step", "fmt", "--step", "clippy", "--step", "test"], "clippy", "stop");
    assert_eq!(code, Some(1));
    assert_eq!(
        normalise(&out),
        ["==> fmt", "fmt: ok (Ts)", "==> clippy", "clippy: FAILED", "ci: FAILED (clippy)"]
    );
    assert_eq!(log, "fmt --all --check\nclippy --all-targets -- -D warnings\n");
}

// §2: `--keep-going` runs every step and names every failure.
#[test]
fn s2_keep_going() {
    let (code, out, _) = run_binary(
        &["--keep-going", "--step", "fmt", "--step", "test", "--step", "transform"],
        "--check",
        "keep",
    );
    assert_eq!(code, Some(1));
    assert_eq!(
        normalise(&out),
        [
            "==> fmt",
            "fmt: FAILED",
            "==> test",
            "test: ok (Ts)",
            "==> transform",
            "transform: no transform tests yet (spec/harness.md §6)",
            "transform: ok (Ts)",
            "ci: FAILED (fmt)",
        ]
    );
}

// §2: the full gate, over a fixture so the `lint` step is deterministic:
// the order of all six steps and the `kiln` step's two commands.
#[test]
fn s2_full_gate_order() {
    let (f, cargo, log) = fake("full");
    // The fake reads its log and failure word from the environment of the
    // process; give them through a wrapper instead of this process's env.
    f.write(
        "bin/wrap",
        &format!(
            "#!/bin/sh\nFAKE_CARGO_LOG='{}' FAKE_CARGO_FAIL='' exec '{}' \"$@\"\n",
            log.display(),
            cargo.display()
        ),
    );
    let wrap = f.path("bin/wrap");
    std::fs::set_permissions(&wrap, std::fs::Permissions::from_mode(0o755)).unwrap();
    // The wrapper is a script just written, as the fake is: without this
    // its first run can fail with `ETXTBSY` and the gate reports a failed
    // step.
    common::wait_executable(&wrap);
    let opts = ci::parse_args(&[]).unwrap();
    let mut out = Vec::new();
    assert!(ci::run(&opts, f.root(), wrap.as_os_str(), &mut out));
    let out = String::from_utf8(out).unwrap();
    let names: Vec<String> = normalise(&out);
    assert_eq!(
        names,
        [
            "==> lint",
            "lint: ok (Ts)",
            "==> fmt",
            "fmt: ok (Ts)",
            "==> clippy",
            "clippy: ok (Ts)",
            "==> test",
            "test: ok (Ts)",
            "==> kiln",
            "kiln: ok (Ts)",
            "==> transform",
            "transform: no transform tests yet (spec/harness.md §6)",
            "transform: ok (Ts)",
            "ci: ok",
        ]
    );
    let k = "-p slip_kiln -p slip_digest -p slip_script -p slip_cli";
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        format!(
            "fmt --all --check\nclippy --all-targets -- -D warnings\ntest\n\
             clippy {k} --all-targets -- -D warnings\ntest {k}\n"
        )
    );
}

// §2 and §3: the `lint` step reports the violations and fails the gate.
#[test]
fn s2_lint_step_fails_on_violations() {
    let f = Fixture::new("lintstep");
    f.write("docs/x.md", "[a](missing.md)\n");
    let opts = Options { steps: vec!["lint"], keep_going: false };
    let mut out = Vec::new();
    assert!(!ci::run(&opts, f.root(), "/nonexistent/cargo".as_ref(), &mut out));
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "==> lint\ndocs/x.md:1: doc-links: link `missing.md` points at no file\nlint: FAILED\n\
         ci: FAILED (lint)\n"
    );
}

// §2: a cargo that cannot be started fails its step.
#[test]
fn s2_missing_cargo_fails_the_step() {
    let f = Fixture::new("nocargo");
    let opts = Options { steps: vec!["fmt"], keep_going: false };
    let mut out = Vec::new();
    assert!(!ci::run(&opts, f.root(), "/nonexistent/cargo".as_ref(), &mut out));
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.starts_with("==> fmt\ncannot run `/nonexistent/cargo fmt --all --check`: "),
        "{out}"
    );
    assert!(out.ends_with("fmt: FAILED\nci: FAILED (fmt)\n"), "{out}");
}

// WP-090 (spec/landed/api/WP-090.md): `SLIP_BLESS` set, to any value, the
// empty one too, fails the gate before any step runs: the script tests
// would write their `expect.txt` and pass whatever the host renders.
#[test]
fn wp090_the_gate_never_blesses() {
    assert_eq!(ci::bless_refusal(None), None);
    for value in ["1", "", "0"] {
        let (_f, cargo, log) = fake("bless");
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["ci", "--step", "test"])
            .env("CARGO", &cargo)
            .env("FAKE_CARGO_LOG", &log)
            .env("FAKE_CARGO_FAIL", "")
            .env("SLIP_BLESS", value)
            .output()
            .unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(1), "SLIP_BLESS={value:?}: {stdout}");
        assert_eq!(
            stdout,
            format!(
                "{}\nci: FAILED (SLIP_BLESS is set)\n",
                ci::bless_refusal(Some(value.as_ref())).unwrap()
            )
        );
        assert!(stdout.starts_with(&format!("ci: SLIP_BLESS={value} is set;")), "{stdout}");
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), "", "no step ran");
    }
}
