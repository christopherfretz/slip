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
pub fn records(input: &Input<'_>, rules: &RuleTable) -> Result<Vec<Record>, String> {
    let options = Options { rules, detail: Detail::None, names: true };
    walk::run_with(input, &options, |w, written, output| collect(w, written, &output))
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
                KeyRes::Key { .. } => match key_again(w, e) {
                    Some(k) => Some(k),
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
fn key_again(w: &mut Walk<'_>, d: DeclId) -> Option<Key> {
    let e = w.entity(d);
    if !w.enter() {
        return None;
    }
    let fresh = w.keys_in_progress.insert(e);
    let saved = w.take_unit();
    w.in_key += 1;
    let computed = w.compute_key(w.representative(e));
    w.in_key -= 1;
    w.unit = saved;
    if fresh {
        w.keys_in_progress.remove(&e);
    }
    w.leave();
    computed.ok().map(|(key, _)| w.rules.map(key))
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
