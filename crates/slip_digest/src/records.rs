//! Unit records (`spec/verify.md` §2.3 "Records"): for every unit of a
//! translation unit, its `KeyId`, the `Key` behind it, its kind, class and
//! name, and whether it is indeterminate. What `slip declare` resolves
//! names in. Work package WP-055.
//!
//! Trusted base only in that it must not disturb the walk: the records are
//! read after the walk has produced its `Output` (`walk::run_with`), and
//! the one function called after it that computes anything is
//! `Walk::compute_key`, whose discriminator streams are its own and are
//! hashed into no unit. A `Key` computed again is checked against the
//! unit's `KeyId`: a record is never guessed.

use std::collections::BTreeSet;

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::ids::DeclId;
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, KeyKind, Name, Parent, RuleTable};

use crate::keys::KeyRes;
use crate::units::Written;
use crate::walk::{self, Walk};
use crate::{Detail, Input, Options, Output};

/// One unit of an image, as `slip declare` reads it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Record {
    /// The unit's `KeyId`, as `digest` gives it under `rules`.
    pub id: Digest,
    /// The Key whose id that is; `None` for a fallback key.
    pub key: Option<Key>,
    pub kind: KeyKind,
    pub class: GroupClass,
    /// `spec/verify.md` §6.1.
    pub name: String,
    pub indeterminate: bool,
}

/// One record per unit that `digest(input, …)` has under `rules`, sorted by
/// `(id, name)`. `Err`, naming the unit, if a unit that has no fallback key
/// gets a `Key` whose id is not the unit's `KeyId`: no record is guessed.
///
/// The walk is the ordinary one (detail `none`, names asked for). Units
/// that give the same record (two copies of one content-keyed declaration,
/// two equal namespaces merged by a rule) are one record; units of one key
/// with different names or kinds (a collision) are a record each, sorted
/// further by class, kind, the flag and the `Key`.
///
/// `read_only` is the walk's ([`Options::read_only`]): under a non-empty
/// table the records are the units of the walk that has it.
pub fn records(
    input: &Input<'_>,
    rules: &RuleTable,
    read_only: crate::ReadOnly<'_>,
) -> Result<Vec<Record>, String> {
    let options = Options { rules, read_only, detail: Detail::None, names: true, rule_use: false };
    walk::run_with(input, &options, |w, written, output| collect(w, written, &output))
}

/// What `slip declare` checks its `relocate` lines with
/// (`spec/verify.md` §2.3, WP-111), by unit `KeyId` under the empty table
/// (the ids [`records`] gives with no rule).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// Units whose entity is a dependency entity under the run's files
    /// (`spec/verify.md` §2.1, rule 1: no rule matches it).
    pub dependency: BTreeSet<Digest>,
    /// Units of a friend declaration of a function, or function template,
    /// in a templated class (its own entity, keyed in that class) that
    /// does not define it: such a declaration is not followed to the
    /// function's rule and needs one of its own (`spec/landed/digest/
    /// WP-103.md`, fix round 3, finding 2).
    pub undefined_friends: BTreeSet<Digest>,
}

impl Facts {
    /// The facts of several images as one: each set the union, so that
    /// an entity that is a dependency entity in any observer is one
    /// (knowing less never accepts more).
    pub fn merge(&mut self, other: Facts) {
        self.dependency.extend(other.dependency);
        self.undefined_friends.extend(other.undefined_friends);
    }
}

/// The [`Facts`] of `input` under the run's files `read_only`. The walk
/// is the one [`records`] runs under the empty table; ownership, which
/// does not depend on the table, is asked after it.
pub fn facts(input: &Input<'_>, read_only: crate::ReadOnly<'_>) -> Facts {
    let empty = RuleTable::empty();
    let options =
        Options { rules: &empty, read_only, detail: Detail::None, names: false, rule_use: false };
    walk::run_with(input, &options, |w, written, _| {
        w.owned_state.query = true;
        let mut out = Facts::default();
        for u in written {
            let Some(e) = u.entity else { continue };
            if !w.owned(e) {
                out.dependency.insert(u.key);
            }
            if w.friend_class(e).is_some()
                && !crate::units::instantiated_outside_class(w, e)
                && !defines(w, e)
            {
                out.undefined_friends.insert(u.key);
            }
        }
        out
    })
}

/// Whether a declaration of the function `e` (for a function template,
/// of its pattern) has a body.
fn defines(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let f = if ast.decls.get(e).kind() == DeclKind::FunctionTemplate {
        ast.template_info(e).and_then(|i| i.pattern)
    } else {
        Some(e)
    };
    f.and_then(|f| ast.definition(f))
        .and_then(|d| ast.decls.function(d))
        .is_some_and(|x| x.body.is_some())
}

/// A property of the walk under ownership (WP-110, `spec/verify.md`
/// §2.1 rule 2), in two parts. `Ok` is the number of keys compared.
///
/// 1. **Order independence.** Every key the walk memoised (each written
///    by some stream that refers to the entity, or by the entity's own
///    unit) is computed again in a fresh walk, set up as `walk::run_with`
///    sets it up, in the reverse order of entities: a key that depended
///    on which reference asked first would differ.
/// 2. **The namespace views against an oracle.** A namespace keyed in
///    the owned view (`true`) is the key a walk with every file owned
///    (`ReadOnly::NO_FILE`) gives it, rules applied along the whole
///    chain; in the dependency view (`false`), the key of the empty
///    table, no rule anywhere on the chain. Checked for every view the
///    walk memoised and for every namespace's own key, by its own
///    ownership. A namespace the oracle does not own (a declaration in a
///    buffer that is no file: read-only under every ownership) is not
///    checked in the owned view. Under a table that holds a refused rule
///    only part 1 runs.
pub fn key_agreement(
    input: &Input<'_>,
    rules: &RuleTable,
    read_only: crate::ReadOnly<'_>,
) -> Result<usize, String> {
    let options = Options { rules, read_only, detail: Detail::None, names: false, rule_use: false };
    let (memo, views) = walk::run_with(input, &options, |w, _, _| {
        let mut memo: Vec<(DeclId, KeyRes)> = w.keys.iter().map(|(&e, &r)| (e, r)).collect();
        memo.sort_by_key(|&(e, _)| e.index());
        // Every namespace's own key, in the view of its own ownership.
        let mut views: Vec<(DeclId, bool, KeyRes)> = Vec::new();
        if !w.rules.is_empty() {
            for &(e, r) in &memo {
                if w.ast.decls.get(e).kind() == DeclKind::Namespace {
                    let o = w.owned(e);
                    views.push((e, o, r));
                }
            }
        }
        views.extend(w.owned_state.views.iter().map(|(&(n, b), &r)| (n, b, r)));
        views.sort_by_key(|&(n, b, _)| (n.index(), b));
        (memo, views)
    });
    let name = |e: DeclId| crate::names::entity_name(input.ast, e, KeyKind::Class);
    let mut w = fresh(input, read_only, rules);
    for &(e, r) in memo.iter().rev() {
        let again = w.key(e);
        if again != r {
            return Err(format!(
                "the key of {} ({e:?}) is {r:?} in the walk and {again:?} alone",
                name(e)
            ));
        }
    }
    // Under a table that holds a refused rule nothing is keyed
    // (`RuleTable::refused`): the oracle's walks would key.
    if rules.refused().is_some() {
        return Ok(memo.len());
    }
    let empty = RuleTable::empty();
    let mut all = fresh(input, crate::ReadOnly::NO_FILE, rules);
    let mut none = fresh(input, crate::ReadOnly::EVERY_FILE, &empty);
    for &(n, b, r) in &views {
        let expected = if b {
            if !all.owned(n) {
                continue;
            }
            all.key(n)
        } else {
            none.key(n)
        };
        if expected != r {
            let view = if b { "owned" } else { "dependency" };
            return Err(format!(
                "the namespace {} ({n:?}) in the {view} view is {r:?} in the walk and {expected:?} by the oracle",
                name(n)
            ));
        }
    }
    Ok(memo.len() + views.len())
}

/// A walk of `input` under `table` and `ro`, its units collected and
/// its locals numbered, as `key_agreement` recomputes keys in.
fn fresh<'a>(input: &Input<'a>, ro: crate::ReadOnly<'a>, table: &'a RuleTable) -> Walk<'a> {
    let mut w = Walk::new(input, table);
    w.read_only = ro;
    crate::units::collect(&mut w);
    crate::local::number_all(&mut w);
    w
}

fn collect(w: &mut Walk<'_>, written: &[Written], output: &Output) -> Result<Vec<Record>, String> {
    let mut out = Vec::with_capacity(written.len());
    for u in written {
        let name = name_of(w, u, output)?;
        let key = match u.entity {
            // The order unit: the key `units::order_unit` builds, mapped.
            None => Some(w.rules.map(Key {
                kind: KeyKind::Order,
                parent: Parent::Root,
                name: Name::Order,
                disc: None,
            })),
            Some(e) => match w.key(e) {
                // No key at all: the unit was written under a fallback key.
                KeyRes::None(_) => None,
                KeyRes::Key { .. } => match key_again(w, e, u.view) {
                    // A resolved or `default_argument` unit is keyed
                    // under its entity's key (digest-kinds §11.1, §11.4).
                    Some(k) => Some(crate::demand::part_key(w, k.clone(), u).unwrap_or(k)),
                    None => {
                        return Err(format!(
                            "the unit `{name}` ({} {}) has a key, but computed again it has none",
                            u.kind.name(),
                            u.key
                        ));
                    }
                },
            },
        };
        if let Some(k) = &key {
            let id = k.id();
            if id != u.key {
                return Err(format!(
                    "the unit `{name}` ({} {}) has a key whose id, computed again, is {id}",
                    u.kind.name(),
                    u.key
                ));
            }
        }
        out.push(Record {
            id: u.key,
            key,
            kind: u.kind,
            class: u.class,
            name,
            indeterminate: u.cause.is_some(),
        });
    }
    out.sort_by(|a, b| {
        (a.id, &a.name, a.class, a.kind, a.indeterminate, &a.key).cmp(&(
            b.id,
            &b.name,
            b.class,
            b.kind,
            b.indeterminate,
            &b.key,
        ))
    });
    out.dedup();
    Ok(out)
}

/// The unit's name, as `Output::names` has it: the names of the walk are
/// computed per unit, so the one of this unit is computed alone and must
/// be among them.
fn name_of(w: &Walk<'_>, u: &Written, output: &Output) -> Result<String, String> {
    let one = crate::names::names(w, std::slice::from_ref(u));
    let Some((_, name)) = one.into_iter().next() else {
        return Err(format!("the unit {} has no name", u.key));
    };
    if output
        .names
        .binary_search_by(|(k, n)| (k, n.as_str()).cmp(&(&u.key, name.as_str())))
        .is_err()
    {
        return Err(format!(
            "the name `{name}` of the unit {} is not among the walk's names",
            u.key
        ));
    }
    Ok(name)
}

/// The mapped `Key` of entity `e`, computed again inside the wrapper
/// `Walk::key` uses: the depth guard, the entity marked in progress, the
/// unit's state put aside, `in_key`, and the entity's representative as
/// the declaration read. `None` when it has none (or the depth guard
/// refuses).
///
/// A namespace's unit in a view that is not its own (`view`,
/// `units::view_units`, WP-140) has the key of that view, computed again
/// the same way.
pub(crate) fn key_again(w: &mut Walk<'_>, d: DeclId, view: Option<bool>) -> Option<Key> {
    let e = w.entity(d);
    if !w.enter() {
        return None;
    }
    let fresh = w.keys_in_progress.insert(e);
    let saved = w.take_unit();
    w.in_key += 1;
    let r = w.representative(e);
    let computed = match view {
        None => w.compute_key(r),
        Some(owned_view) => w.compute_view_key(r, owned_view),
    };
    w.in_key -= 1;
    w.unit = saved;
    if fresh {
        w.keys_in_progress.remove(&e);
    }
    w.leave();
    computed.ok().map(|(key, _)| w.map_key(e, view, key))
}

#[cfg(test)]
mod tests {
    //! digest §10, `spec/verify.md` §2.3 `Fallback`: no source in the pair
    //! tests gives a unit a fallback key (the walk keys every unit it
    //! writes), so the case is made as `tests::a_unit_without_a_key_is_kept`
    //! makes it: the key of `m` is put in the memo as none.

    use slip_kiln::kiln::ast::decl::DeclKind;
    use slip_kiln::kiln::ast::names::DeclNameView;
    use slip_model::digests::Digests;
    use slip_model::keys::{Key, KeyKind, Name, Parent, RuleTable};

    use super::collect;
    use crate::keys::KeyRes;
    use crate::units::{self, UnitClass};
    use crate::walk::Walk;
    use crate::{Cause, Input, Output};

    fn named(w: &Walk<'_>, name: &str, kind: DeclKind) -> slip_kiln::kiln::ast::DeclId {
        let ast = w.ast;
        let found = ast.decls.ids().find(|&d| {
            let n = ast.decls.get(d);
            let is = matches!(ast.names.view(n.name()), DeclNameView::Identifier(s) if ast.symbols().as_str(s) == name);
            n.kind() == kind && is
        });
        found.expect("the declaration")
    }

    /// Parses `struct S { int m; };`, writes the units of `m` and `S`, and
    /// gives `f` the walk, the ids of `S` and `m`, and the written units.
    fn with_units<R>(
        f: impl FnOnce(&mut Walk<'_>, [slip_kiln::kiln::ast::DeclId; 2], &mut Vec<units::Written>) -> R,
    ) -> R {
        let files = [("main.cpp", "struct S { int m; };\n")];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            let s = w.entity(named(&w, "S", DeclKind::CXXRecord));
            let m = w.entity(named(&w, "m", DeclKind::Field));
            let mut written = Vec::new();
            f(&mut w, [s, m], &mut written)
        })
        .expect("the snippet parses")
    }

    fn output_of(w: &Walk<'_>, written: &[units::Written]) -> Output {
        let mut names = crate::names::names(w, written);
        names.sort();
        Output {
            digests: Digests { scheme: 0, tu: slip_model::Digest([0; 32]), groups: Vec::new() },
            counters: Vec::new(),
            causes: Vec::new(),
            names,
            rule_use: Vec::new(),
            kept: Vec::new(),
            name_strings: Vec::new(),
            made: Vec::new(),
            fixed: Vec::new(),
        }
    }

    // `records` never guesses: a unit written under a `KeyId` that its
    // `Key`, computed again, does not have is an `Err` naming the unit.
    #[test]
    fn a_unit_whose_key_has_another_id_is_err() {
        with_units(|w, [_, m], written| {
            let KeyRes::Key { kind, cause, .. } = w.key(m) else { panic!("m has a key") };
            let wrong = slip_model::Digest([0xee; 32]);
            w.keys.insert(m, KeyRes::Key { id: wrong, kind, cause });
            written.push(units::write(w, m, UnitClass::Declared).expect("m written"));
            assert_eq!(written[0].key, wrong);
            let output = output_of(w, written);
            let e = collect(w, written, &output).expect_err("a wrong id is an error");
            assert!(e.contains("`S::m`") && e.contains(&wrong.to_string()), "{e}");
            assert!(e.contains("computed again, is"), "{e}");
        });
    }

    // And one whose `Key` cannot be computed again (here: the key of its
    // parent is in progress, a cycle) is an `Err` naming it.
    #[test]
    fn a_unit_whose_key_cannot_be_computed_again_is_err() {
        with_units(|w, [s, m], written| {
            written.push(units::write(w, m, UnitClass::Declared).expect("m written"));
            let output = output_of(w, written);
            w.keys.remove(&s);
            w.keys_in_progress.insert(s);
            let e = collect(w, written, &output).expect_err("no key computed again is an error");
            assert!(e.contains("`S::m`") && e.contains("computed again it has none"), "{e}");
        });
    }

    #[test]
    fn a_fallback_key_has_no_key() {
        let files = [("main.cpp", "struct S { int m; };\n")];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            let s = w.entity(named(&w, "S", DeclKind::CXXRecord));
            let m = w.entity(named(&w, "m", DeclKind::Field));
            let KeyRes::Key { id: parent, .. } = w.key(s) else { panic!("S has a key") };
            w.keys.insert(m, KeyRes::None(Some(Cause::Unaccounted("key/test"))));
            let um = units::write(&mut w, m, UnitClass::Declared).expect("m written");
            let us = units::write(&mut w, s, UnitClass::Declared).expect("S written");
            let written = [um, us];
            let mut names = crate::names::names(&w, &written);
            names.sort();
            let output = Output {
                digests: Digests { scheme: 0, tu: slip_model::Digest([0; 32]), groups: Vec::new() },
                counters: Vec::new(),
                causes: Vec::new(),
                names,
                rule_use: Vec::new(),
                kept: Vec::new(),
                name_strings: Vec::new(),
                made: Vec::new(),
                fixed: Vec::new(),
            };
            let recs = collect(&mut w, &written, &output).expect("records");
            assert_eq!(recs.len(), 2, "{recs:#?}");
            let rm = recs.iter().find(|r| r.kind == KeyKind::Field).expect("m");
            assert_eq!(rm.key, None, "a fallback key is no Key");
            assert!(rm.indeterminate);
            let want = Key {
                kind: KeyKind::Field,
                parent: Parent::Unit(parent),
                name: Name::Content,
                disc: Some(written[0].digest),
            };
            assert_eq!(rm.id, want.id(), "its id is the unit's (fallback) key");
            let rs = recs.iter().find(|r| r.kind == KeyKind::Class).expect("S");
            assert_eq!(rs.key.as_ref().map(Key::id), Some(parent));
        })
        .expect("the snippet parses");
    }
}
