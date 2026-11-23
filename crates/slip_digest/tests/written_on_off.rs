//! `slip_kiln::ParseOptions::written` changes nothing the digest reads
//! (work package WP-067, fix round 1; `spec/model.md` §8.4): over a unit
//! with standard headers, macros, templates, default arguments and an
//! inheriting constructor, the declaration arena, the shard and the
//! `slip_digest::digest` output are the same with the written-reference
//! and declaration-extent tables recorded and without them.
//!
//! The parse is the production one (`parse_observer_with`), in a tree
//! under the target directory; `slip_kiln`'s own test of this
//! (`crates/slip_kiln/tests/written.rs`) cannot call the digest.

use std::path::PathBuf;

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
fn wp067_written_changes_no_digest() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("written-on-off");
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
    assert!(on_digest == off_digest, "the digest output differs");
    let _ = std::fs::remove_dir_all(&dir);
}
