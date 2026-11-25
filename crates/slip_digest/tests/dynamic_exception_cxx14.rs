//! A function's own dynamic exception specification before C++17
//! (work package WP-041, fix round 2, R3; digest-kinds §4.2 rule 3):
//! `throw(int)` against `throw(long)` under `-std=c++14`, where Kiln's
//! code generator emits the types as a run-time filter, differ; the same
//! specification twice is equal. The pair-test harness parses C++17 and
//! later only (`slip_kiln::snippet`), so this is the production parse
//! (`parse_observer_with`) of a tree under the work root (`common::work_root()`).

mod common;

use slip_digest::{Detail, Input, Options};
use slip_kiln::ParseOptions;
use slip_kiln::tree::Tree;
use slip_model::keys::RuleTable;
use slip_model::{Digest, Status, digests::Unit};

/// The digest and determinacy of the unit named `name` in `source`,
/// parsed as C++14.
fn unit_of(label: &str, source: &str, name: &str) -> (Digest, bool) {
    let tree = Tree::new(&common::work_root(), label, &[("main.cpp", source.as_bytes())])
        .expect("the tree");
    let observer = tree.observer("main.cpp", "c++14", &[]);
    let (shard, got) = tree
        .parse(&observer, &ParseOptions::default(), |unit| {
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
                        let o = slip_digest::digest(&input, &options);
                        let key = o
                            .names
                            .iter()
                            .find(|(_, n)| n == name)
                            .map(|(k, _)| *k)
                            .expect("the unit");
                        let u: &Unit = o
                            .digests
                            .groups
                            .iter()
                            .flat_map(|g| g.units.iter().flatten())
                            .find(|u| u.key == key)
                            .expect("its digest");
                        let indeterminate = o.causes.iter().any(|(k, _)| *k == key);
                        (u.digest, indeterminate)
                    })
                    .expect("the digest thread")
                    .join()
                    .expect("the digest")
            })
        })
        .expect("the unit parses");
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    got.expect("the closure ran")
}

const INT: &str = "void h();\nvoid g() throw(int) { h(); }\n";
const LONG: &str = "void h();\nvoid g() throw(long) { h(); }\n";

#[test]
fn an_own_dynamic_exception_specification_is_written_before_cxx17() {
    let a = unit_of("wp041-dyn14-int", INT, "g()");
    let b = unit_of("wp041-dyn14-long", LONG, "g()");
    let again = unit_of("wp041-dyn14-int-again", INT, "g()");
    assert!(!a.1 && !b.1, "both determinate: {a:?} {b:?}");
    assert_ne!(a.0, b.0, "throw(int) against throw(long)");
    assert_eq!(a.0, again.0, "the same specification twice");
}
