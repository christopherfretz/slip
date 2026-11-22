//! Demand independence (`spec/digest.md` §14): every key in both sides has
//! the same digests and flags, and the declared keys only B has are
//! exactly the `added` lines (none if there are none); A has no declared
//! key B lacks. The one other line allowed is `changed <order>`: an added
//! variable definition is listed by the order unit (digest §2.1), which
//! then differs, and must (WP-033).

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{Expect, Side};
use slip_model::digests::GroupClass;

fn one(dir: &Path) -> Result<(), String> {
    let expect = common::expect(dir)?;
    let a = common::digest_side(dir, Side::A)?;
    let b = common::digest_side(dir, Side::B)?;
    let mut problems = Vec::new();
    for (side, d) in [("A", &a), ("B", &b)] {
        for u in common::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
    }
    let order = Expect::Changed("<order>".to_owned());
    let order_changes = expect.contains(&order);
    for (k, ua) in &a.units {
        let name = a.name(k);
        let differs = b.units.get(k).is_some_and(|ub| ua != ub);
        if name == "<order>" && order_changes {
            if !differs {
                problems.push("expected but not seen: changed <order>".to_owned());
            }
        } else if differs {
            problems.push(format!("differs: {name}"));
        }
    }
    if let Some(e) = expect.iter().find(|e| !matches!(e, Expect::Added(_)) && **e != order) {
        return Err(format!("{e:?}: a demand test has `added` lines and `changed <order>` only"));
    }
    let added: BTreeSet<Expect> = expect.into_iter().filter(|e| *e != order).collect();
    let declared = |d: &common::Digested, other: &common::Digested| -> BTreeSet<String> {
        d.units
            .iter()
            .filter(|(k, v)| {
                !other.units.contains_key(*k) && v.iter().any(|u| u.class == GroupClass::Declared)
            })
            .map(|(k, _)| d.name(k))
            .collect()
    };
    let got: BTreeSet<Expect> = declared(&b, &a).into_iter().map(Expect::Added).collect();
    if got != added {
        problems.push(format!("declared keys only B has: expected {added:?}, saw {got:?}"));
    }
    let only_a = declared(&a, &b);
    if !only_a.is_empty() {
        problems.push(format!("declared keys only A has: {only_a:?}"));
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

#[test]
fn demand() {
    common::run_all("demand", one);
}
