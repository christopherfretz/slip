//! `cargo xtask wp [--ready]` (spec/harness.md §4).

mod common;

use common::Fixture;
use std::process::Command;
use xtask::repo::Repo;
use xtask::wp;

fn write_wp(f: &Fixture, id: &str, status: &str, needs: &str, title: &str) {
    f.write(
        &format!("docs/wp/{id}.md"),
        &format!(
            "# {id} — {title}\n\n- Status: {status}\n- Milestone: M1\n- Track: G\n\
             - Needs: {needs}\n- Size: S\n"
        ),
    );
}

// §4: the table, ordered by id (numerically: WP-999 before WP-1000).
#[test]
fn s4_table_ordered_by_id() {
    let f = Fixture::new("wptable");
    write_wp(&f, "WP-1000", "review", "WP-001, WP-999", "Last");
    write_wp(&f, "WP-999", "in-progress", "none", "Nine");
    let repo = Repo::open(f.root()).unwrap();
    let wps = wp::load(&repo).unwrap();
    let all: Vec<&wp::WorkPackage> = wps.iter().collect();
    assert_eq!(
        wp::table(&all),
        "\
id       status       milestone  track  size  needs           title
WP-001   done         M0         A      M     none            slip_model: wire
WP-002   todo         M0         D      S     WP-001          slip_cdb: split
WP-999   in-progress  M1         G      S     none            Nine
WP-1000  review       M1         G      S     WP-001, WP-999  Last
"
    );
}

// §4 `--ready`: status `todo` and every need `done`.
#[test]
fn s4_ready() {
    let f = Fixture::new("wpready");
    write_wp(&f, "WP-003", "todo", "none", "No needs");
    write_wp(&f, "WP-004", "todo", "WP-001, WP-002", "Needs a todo");
    write_wp(&f, "WP-005", "review", "none", "Not todo");
    let repo = Repo::open(f.root()).unwrap();
    let wps = wp::load(&repo).unwrap();
    let ready: Vec<&str> = wp::ready(&wps).iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ready, ["WP-002", "WP-003"]);
}

// §4: a malformed file is reported, not shown.
#[test]
fn s4_malformed_file_is_an_error() {
    let f = Fixture::new("wpbad");
    f.write("docs/wp/WP-003.md", "# WP-003 — x\n\n- Status: maybe\n");
    let repo = Repo::open(f.root()).unwrap();
    let errs = wp::load(&repo).unwrap_err();
    assert_eq!(
        errs[0].to_string(),
        "docs/wp/WP-003.md:3: wp: `Status` is `maybe`, not one of todo, in-progress, review, \
         done, dropped"
    );
}

// §4: the command on this repository prints the header and exits 0;
// unknown arguments are a usage error.
#[test]
fn s4_binary() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask")).args(["wp", "--ready"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let header: Vec<&str> = text.lines().next().unwrap().split_whitespace().collect();
    assert_eq!(header, ["id", "status", "milestone", "track", "size", "needs", "title"]);
    let out = Command::new(env!("CARGO_BIN_EXE_xtask")).args(["wp", "--all"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}
