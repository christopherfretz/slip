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
/// parsed as C++14, and the translation unit's count of site-1 operands
/// (`digest.operands.site.1`; 0 when absent).
fn unit_of(label: &str, source: &str, name: &str) -> (Digest, bool, u64) {
    let tree = Tree::new(&common::work_root(), label, &[("main.cpp", source.as_bytes())])
        .expect("the tree");
    let observer = tree.observer("main.cpp", "c++14", &[]);
    // Every digested parse records the written-reference table (L0).
    let mut parse = ParseOptions::default();
    parse.written = true;
    let (shard, got) = tree
        .parse(&observer, &parse, |unit| {
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
                        let site1 = o
                            .counters
                            .iter()
                            .find(|c| c.name == "digest.operands.site.1")
                            .map_or(0, |c| c.value);
                        (u.digest, indeterminate, site1)
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

/// WP-156 fix round 3, A5 (Layer S): a dynamic exception specification's
/// types are written types, so `decltype(make(0))` in `throw(...)` is a
/// site-1 operand of the function's operand unit. `make(0)` binds
/// `make(int)` when it is declared before `g`, `make(long)` when after;
/// both are `int`, so only the operand tells them apart. L3 would write
/// `make` at site 9 if no stream walked it, so the operand's own count
/// is what shows Layer S read it (mutation 1 of the round: the types not
/// passed to a `Type` slot).
#[test]
fn a5_a_dynamic_exception_specification_holds_its_operands() {
    const BEFORE: &str = "int make(long);\nint make(int);\nvoid g() throw(decltype(make(0)));\n";
    const AFTER: &str = "int make(long);\nvoid g() throw(decltype(make(0)));\nint make(int);\n";
    let a = unit_of("wp156-a5-before", BEFORE, "g()::<operands>");
    let b = unit_of("wp156-a5-after", AFTER, "g()::<operands>");
    let again = unit_of("wp156-a5-before-again", BEFORE, "g()::<operands>");
    assert!(!a.1 && !b.1, "both determinate: {a:?} {b:?}");
    assert_ne!(a.0, b.0, "make(int) against make(long)");
    assert_eq!(a.0, again.0, "the same binding twice");
    assert_eq!((a.2, b.2), (1, 1), "the dynamic specification's operand is read (site 1)");
}
