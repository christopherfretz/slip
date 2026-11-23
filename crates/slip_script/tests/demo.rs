//! The demo transform of `docs/demos/demo-3.md` part A
//! (`resource/examples/swap_arguments.cpp`, `spec/api.md` §10.1) through
//! the driver, and the thread a loaded transform runs on (`spec/api.md`
//! §2.5 rule 5): only the loading thread.

mod common;

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

// §2.5 rule 5: on the loading thread the demo's run completes with the
// three edits of the one call, twice alike. A run on another thread is
// refused with the rule's text, does not reach Kiln (no host call, no
// counters) and retires nothing: the next run at home completes again.
#[test]
fn s2_5_rule5_a_run_away_from_the_loading_thread_is_refused() {
    let s = subject();
    let file = example();
    let res = resource();
    let (home, away, again, retired) = std::thread::scope(|t| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(t, || {
                driver::with_transform(&res, &[], &file, driver::STACK, |l| {
                    let l = &mut l.unwrap_or_else(|e| panic!("{e}"));
                    let home = l.run(&s, Budgets::default(), &Cancel::new());
                    let away = std::thread::scope(|t| {
                        std::thread::Builder::new()
                            .stack_size(STACK)
                            .spawn_scoped(t, || l.run(&s, Budgets::default(), &Cancel::new()))
                            .unwrap()
                            .join()
                            .unwrap()
                    });
                    let again = l.run(&s, Budgets::default(), &Cancel::new());
                    (home, away, again, l.retired().map(str::to_owned))
                })
            })
            .unwrap()
            .join()
            .unwrap()
    });
    assert_eq!(edits(&home), 3, "{home:?}");
    assert_eq!(away, Err(driver::OTHER_THREAD.to_owned()));
    assert_eq!(edits(&again), 3, "{again:?}");
    assert_eq!(home.unwrap().0, again.unwrap().0);
    assert_eq!(retired, None);
}
