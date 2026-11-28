//! The ledger of written names (WP-156 fix round 3, Layer L: L1, L2, L6):
//! Kiln's written-reference table (Kiln `spec/ast.md` §18) indexed by the
//! unit that holds each entry, and what each unit's `finish` decided of
//! it. The decisions are [`super::finish`]'s (L3, L5); the sweep after
//! the queue is drained is [`Ledger::sweep`].

use std::collections::HashMap;

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::written_refs::{
    REF_DEPENDENT, REF_UNRESOLVED, RefRole, WrittenRef, WrittenRefId,
};
use slip_kiln::kiln::ast::{Ast, DeclId};

use crate::walk::Walk;

/// The part of a unit an entry is deferred to (L3): the part's stream,
/// not the unit's own, walks the expression of its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartOf {
    /// The resolved unit `exception_spec` (digest-kinds §11.2): the
    /// `noexcept` operand of a function whose specification is
    /// deferrable.
    ExceptionSpec,
    /// The `default_argument` unit of a parameter, by index (0 for a
    /// field; digest-kinds §11.4).
    DefaultArgument(u64),
    /// Any other part: nothing is deferred to it.
    Other,
}

/// What the units decided of one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Claim {
    /// No `finish` of its unit has run.
    Open,
    /// Decided by its unit's `finish`: met, or written at site 9.
    Claimed,
    /// Its unit's `finish` left it to the part.
    Deferred(PartOf),
    /// Decided by the part's `finish`.
    ByPart(PartOf),
}

/// One entry that takes part (L1), under one unit that holds it (L2).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Row {
    /// The unit's entity; `None` for the translation unit's `<order>`.
    pub unit: Option<DeclId>,
    pub entry: WrittenRefId,
    /// The entry's entity.
    pub entity: DeclId,
    /// Its role is one L3 joins (`Expr`, `Member`, `Operator`,
    /// `UdSuffix`).
    pub joined: bool,
    pub claim: Claim,
}

/// The index of the translation unit, built once ([`build`]): its rows
/// sorted by unit, then entry (table order).
#[derive(Debug, Default)]
pub(crate) struct Ledger {
    pub rows: Vec<Row>,
}

/// L1: the entity of entry `r` when it takes part: an entry with an
/// entity, neither `DEPENDENT` nor `UNRESOLVED`, whose entity is no
/// namespace and no namespace alias.
pub(crate) fn takes_part(ast: &Ast, r: &WrittenRef) -> Option<DeclId> {
    let e = r.entity?;
    if r.flags & (REF_DEPENDENT | REF_UNRESOLVED) != 0 {
        return None;
    }
    match ast.decls.get(e).kind() {
        DeclKind::Namespace | DeclKind::NamespaceAlias => None,
        _ => Some(e),
    }
}

/// The roles whose names L3 meets by the join of Kiln §18.11.1 and
/// §18.13.7: a name expression, a member access, an operator use, a
/// literal operator's suffix.
pub(crate) fn joined(role: RefRole) -> bool {
    matches!(role, RefRole::Expr | RefRole::Member | RefRole::Operator | RefRole::UdSuffix)
}

/// L2: the rows of the translation unit. An entry's unit is the unit that
/// holds the declaration that holds it (`units::holding_unit`), for each
/// declaration of its declarator group when it is `REF_GROUP` (Kiln
/// §18.3.1); an entry held by the translation unit, or by a declaration
/// no unit holds, is `<order>`'s. Empty when the table is not recorded
/// (L0 makes that a cause).
pub(crate) fn build(w: &mut Walk<'_>) -> Ledger {
    let ast = w.ast;
    let refs = &ast.written_refs;
    let mut rows = Vec::new();
    if !refs.is_on() {
        return Ledger { rows };
    }
    let mut units: HashMap<DeclId, Option<DeclId>> = HashMap::new();
    for (id, r) in refs.iter() {
        let Some(entity) = takes_part(ast, r) else { continue };
        let joined = joined(r.role);
        let row = |unit| Row { unit, entry: id, entity, joined, claim: Claim::Open };
        let holders = refs.holders(id);
        if holders.is_empty() {
            rows.push(row(None));
            continue;
        }
        for &d in holders {
            let u = if let Some(&u) = units.get(&d) {
                u
            } else {
                let u = crate::units::holding_unit(w, d).map(|u| w.entity(u));
                units.insert(d, u);
                u
            };
            rows.push(row(u));
        }
    }
    rows.sort_by_key(|r| (r.unit, r.entry));
    rows.dedup_by_key(|r| (r.unit, r.entry));
    Ledger { rows }
}

impl Ledger {
    /// The index range of the rows of unit `unit`.
    pub(crate) fn of(&self, unit: Option<DeclId>) -> std::ops::Range<usize> {
        let lo = self.rows.partition_point(|r| r.unit < unit);
        let hi = self.rows.partition_point(|r| r.unit <= unit);
        lo..hi
    }

    /// L6: the rows no unit decided (`unclaimed`: their unit was never
    /// written) and the rows deferred to a part that was never written
    /// (`undemanded`: their expression is in no unit, §2.2's own rule).
    pub(crate) fn sweep(&self) -> (u64, u64) {
        let mut unclaimed = 0;
        let mut undemanded = 0;
        for r in &self.rows {
            match r.claim {
                Claim::Open => unclaimed += 1,
                Claim::Deferred(_) => undemanded += 1,
                Claim::Claimed | Claim::ByPart(_) => {}
            }
        }
        (unclaimed, undemanded)
    }
}
