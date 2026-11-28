//! `slip_kiln::ParseOptions::written` changes nothing the digest reads
//! but the ledger's cause (work package WP-067, fix round 1;
//! `spec/model.md` §8.4; WP-156 fix round 3, L0): over a unit with
//! standard headers, macros, templates, default arguments and an
//! inheriting constructor, the declaration arena and the shard are the
//! same with the written-reference and declaration-extent tables recorded
//! and without them, and the `slip_digest::digest` output without them
//! differs from the output with them in operand units alone: the site-9
//! entries of the ledger are written with the table (WP-156 fix round 3,
//! L5), and without it the operand unit of `<order>` is indeterminate
//! with `kiln/written-refs-not-recorded`.
//!
//! The parse is the production one (`parse_observer_with`), in a tree
//! under the work root (`common::work_root()`); `slip_kiln`'s own test of this
//! (`crates/slip_kiln/tests/written.rs`) cannot call the digest.

mod common;

use slip_digest::{Detail, Input, Options};
use slip_kiln::{ParseOptions, parse_observer_with};
use slip_model::keys::RuleTable;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Status};

const HEADER: &str = "\
#pragma once
#define M(x) x + 1
template <class T> struct B { T t; int dm = 3; B() {} B(int) {} void m(int y = 2) {} };
struct Base { Base(int); };
struct Der : Base { using Base::Base; int k = M(2); };
";

const MAIN: &str = "\
#include \"h.h\"
#include <vector>
#include <string>
namespace n { struct S { int a; }; void f(S); }
template <class T> void g(T x) { f(x); }
int main() { B<int> b; b.m(); B<long> c(1); n::S s{1}; g(s); std::vector<std::string> v; \
v.push_back(\"x\"); Der d(1); return M(b.t); }
";

#[test]
fn wp067_written_changes_only_the_ledger_cause() {
    let dir = common::work_root().join("written-on-off");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    std::fs::write(dir.join("h.h"), HEADER).unwrap();
    std::fs::write(dir.join("main.cpp"), MAIN).unwrap();
    let argv =
        ["clang++", "-std=c++20", "main.cpp"].iter().map(|s| s.as_bytes().to_vec()).collect();
    let observer = Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.to_str().unwrap().as_bytes().to_vec(),
            argv,
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let run = |written: bool| {
        let mut options = ParseOptions::default();
        options.written = written;
        let (shard, out) = parse_observer_with(&observer, Phase::Map, 1, options, |unit| {
            let (ast, sm) = (unit.ast(), unit.sm());
            let rules = RuleTable::empty();
            let path = |_: slip_kiln::kiln::source::FileId| Vec::new();
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(slip_digest::STACK_SIZE)
                    .spawn_scoped(scope, || {
                        let input = Input { ast, sm, subject_path: &path };
                        let options = Options {
                            rules: &rules,
                            read_only: slip_digest::ReadOnly::NO_FILE,
                            detail: Detail::All,
                            names: true,
                            rule_use: false,
                        };
                        (slip_digest::digest(&input, &options), ast.decls.len())
                    })
                    .unwrap()
                    .join()
                    .unwrap()
            })
        })
        .unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
        (shard, out.expect("a parse happened"))
    };
    let (off_shard, (off_digest, off_decls)) = run(false);
    let (on_shard, (on_digest, on_decls)) = run(true);
    assert!(off_decls > 10_000, "the standard headers were parsed: {off_decls}");
    assert_eq!(on_decls, off_decls, "the declaration arena");
    assert_eq!(on_shard.to_bytes(), off_shard.to_bytes(), "the shard");
    let units = |o: &slip_digest::Output| {
        let mut v: Vec<([u8; 32], [u8; 32])> = o
            .digests
            .groups
            .iter()
            .flat_map(|g| g.units.iter().flatten())
            .map(|u| (u.key.0, u.digest.0))
            .collect();
        v.sort_unstable();
        v
    };
    // The operand units: with the table, the ledger's site-9 entries
    // (WP-156 fix round 3, L5); without it, none of those, and the
    // operand unit of `<order>` with the ledger's cause (L0).
    let operands = |o: &slip_digest::Output, k: [u8; 32]| {
        o.names.iter().any(|(n, s)| n.0 == k && s.ends_with("::<operands>"))
    };
    let plain = |o: &slip_digest::Output| {
        let mut v = units(o);
        v.retain(|u| !operands(o, u.0));
        v
    };
    assert_eq!(plain(&on_digest), plain(&off_digest), "every unit that is no operand unit");
    let order: Vec<_> =
        off_digest.names.iter().filter(|(_, s)| s == "<order>::<operands>").collect();
    assert_eq!(order.len(), 1, "the operand unit of `<order>` without the table");
    let not_recorded = slip_digest::Cause::Unsupported("kiln/written-refs-not-recorded");
    assert!(
        off_digest.causes.iter().any(|(k, c)| *k == order[0].0 && *c == not_recorded),
        "the ledger's cause: {:?}",
        off_digest.causes
    );
    assert!(on_digest.causes.iter().all(|(_, c)| *c != not_recorded), "{:?}", on_digest.causes);
    assert!(
        on_digest
            .names
            .iter()
            .filter(|(_, s)| !s.ends_with("::<operands>"))
            .all(|n| off_digest.names.contains(n)),
        "the names"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// spec/harness.md §11.5: this crate's work directories follow the rule.
#[test]
fn s11_5_work_root() {
    common::work_root::self_test(env!("CARGO_TARGET_TMPDIR"));
}
