//! The demo transform of `docs/demos/demo-3.md` part A
//! (`resource/examples/swap_arguments.cpp`, `spec/api.md` §10.1) through
//! the driver, run on the compiling thread and away from it, as a worker
//! runs it from a subject unit's front-end thread (`docs/wp/WP-092.md`
//! rule 2): whether a run reaches the stack-guard hazard of
//! `spec/api.md` §2.5 rule 5 (`docs/wp/WP-089.md` "Decided since" item 3).

mod common;

use std::path::Path;

use common::{STACK, resource, subject};
use slip_kiln::session::Cancel;
use slip_script::driver::{self, Budgets, Outcome, RunStats};

fn example() -> std::path::PathBuf {
    resource().join("examples/swap_arguments.cpp")
}

fn edits(r: &Result<(Outcome, RunStats), String>) -> usize {
    match r {
        Ok((Outcome::Completed(out), _)) => out.edits.len(),
        other => panic!("{other:?}"),
    }
}

// §2.5 rule 5: compiled under a 4 MiB guard (as WP-080's s1), the demo's
// run on another thread, then at home, then away again: each completes
// with the three edits of the one call, and Kiln's guard never trips.
#[test]
fn s2_5_rule5_the_demo_runs_away_from_the_compiling_thread() {
    let s = subject();
    let file = example();
    let res = resource();
    let (runs, retired) = std::thread::scope(|t| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(t, || {
                driver::with_transform(&res, &[], &file, 4 << 20, |l| {
                    let l = &mut l.unwrap_or_else(|e| panic!("{e}"));
                    let away = |l: &mut driver::Loaded<'_>| {
                        std::thread::scope(|t| {
                            std::thread::Builder::new()
                                .stack_size(STACK)
                                .spawn_scoped(t, || l.run(&s, Budgets::default(), &Cancel::new()))
                                .unwrap()
                                .join()
                                .unwrap()
                        })
                    };
                    let first = away(l);
                    let home = l.run(&s, Budgets::default(), &Cancel::new());
                    let again = away(l);
                    ([first, home, again], l.retired().map(str::to_owned))
                })
            })
            .unwrap()
            .join()
            .unwrap()
    });
    for r in &runs {
        assert_eq!(edits(r), 3, "{r:?}");
    }
    assert_eq!(runs[0].as_ref().unwrap().0, runs[1].as_ref().unwrap().0);
    assert_eq!(retired, None);
    assert!(Path::new(&file).is_absolute());
}
