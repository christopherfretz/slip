//! Indeterminate tests (`spec/digest.md` §10, §14): side A only; the units
//! flagged for a cause other than `Pending` are exactly the
//! `indeterminate` lines, and none of those has the cause `Pending`. A
//! unit indeterminate only for `Pending` is ignored while `Area` exists.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{Expect, Side};
use slip_digest::Cause;

fn one(dir: &Path) -> Result<(), String> {
    let expect = common::expect(dir)?;
    let want: BTreeSet<String> = expect
        .iter()
        .filter_map(|e| if let Expect::Indeterminate(n) = e { Some(n.clone()) } else { None })
        .collect();
    if want.is_empty() {
        return Err("an indeterminate test needs at least one `indeterminate` line".into());
    }
    let a = common::digest_side(dir, Side::A)?;
    let mut got = BTreeSet::new();
    let mut pending = BTreeSet::new();
    for v in a.units.values() {
        for u in v {
            if u.flags & 1 == 0 {
                continue;
            }
            match u.cause {
                Some(Cause::Pending(_)) | None => {
                    pending.insert(u.name.clone());
                }
                Some(_) => {
                    got.insert(u.name.clone());
                }
            }
        }
    }
    if got == want {
        return Ok(());
    }
    let missing: Vec<_> = want.difference(&got).collect();
    let extra: Vec<_> = got.difference(&want).collect();
    let only_pending: Vec<_> = want.intersection(&pending).collect();
    Err(format!(
        "expected but not flagged: {missing:?} (of which only pending: {only_pending:?})\nflagged but not expected: {extra:?}"
    ))
}

#[test]
fn indeterminate() {
    common::run_all("indeterminate", one);
}
