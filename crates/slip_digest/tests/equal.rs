//! Must-equal pairs (`spec/digest.md` §14): the two sides' `Digests` are
//! equal with `Detail::All`.

mod common;

use std::path::Path;

use common::Side;

fn one(dir: &Path) -> Result<(), String> {
    let a = common::digest_side(dir, Side::A)?;
    let b = common::digest_side(dir, Side::B)?;
    let mut problems = Vec::new();
    for (side, d) in [("A", &a), ("B", &b)] {
        for u in common::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
    }
    if a.output.digests != b.output.digests {
        for (k, ua) in &a.units {
            match b.units.get(k) {
                Some(ub) if ua == ub => {}
                Some(_) => problems.push(format!("differs: {}", a.name(k))),
                None => problems.push(format!("only in A: {}", a.name(k))),
            }
        }
        for k in b.units.keys() {
            if !a.units.contains_key(k) {
                problems.push(format!("only in B: {}", b.name(k)));
            }
        }
        if problems.is_empty() {
            problems.push("the digests differ (groups or files)".into());
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

#[test]
fn equal() {
    common::run_all("equal", one);
}
