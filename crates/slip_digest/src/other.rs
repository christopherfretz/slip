//! The other tables of `Ast` (`spec/digest-kinds.md` §10): the module
//! rows of a declaration's stream (§10.7).

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::modules::{Owner, UnitInfo};

use crate::Cause;
use crate::stream::{FlagWord, Stream};
use crate::tables::unit_kind;
use crate::walk::Walk;

/// `Owner` `unit` names no `UnitInfo` of `ModuleTable` `units`.
const UNIT_MISSING: Cause = Cause::Unaccounted("Owner/unit");

/// The rows `points` and `explicit_defs` of `ModuleTable` for `d`
/// (§10.7), both read from the entity's first declaration (§2.3):
/// `record Owner?`, then `record UnitInfo?` of the unit that holds the
/// explicit instantiation definition. Both are absent when ownership is
/// not tracked (`active` clear).
pub(crate) fn module_rows(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let modules = &ast.modules;
    if !modules.active {
        s.opt(false).opt(false);
        return;
    }
    let first = ast.first(d);
    // points: record Owner?.
    let Owner { unit, purview, exported } = modules.owner(first);
    s.opt(true);
    unit_info(w, s, unit);
    s.flags(FlagWord::new().bit(purview).bit(exported));
    // explicit_defs: record UnitInfo?.
    match modules.explicit_inst_def_unit(first) {
        Some(u) => {
            s.opt(true);
            unit_info(w, s, u);
        }
        None => {
            s.opt(false);
        }
    }
}

/// `record UnitInfo` of unit `u`, written for an `Owner` or for
/// `explicit_defs`: `kind`, `module` and `partition`; no imports (§10.7,
/// rule under the table). `key` and `merged` are `out`.
fn unit_info(w: &Walk<'_>, s: &mut Stream, u: u16) {
    let Some(info) = w.ast.modules.units.get(usize::from(u)) else {
        s.uv(0).indeterminate(UNIT_MISSING);
        return;
    };
    let UnitInfo { key: _, kind, module, partition, imports: _, merged: _ } = info;
    s.tag(unit_kind::row(kind));
    bytes_opt(s, module.as_deref());
    bytes_opt(s, partition.as_deref());
}

/// `bytes?`.
fn bytes_opt(s: &mut Stream, v: Option<&str>) {
    match v {
        Some(v) => {
            s.opt(true).bytes(v.as_bytes());
        }
        None => {
            s.opt(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::decl::DeclKind;

    use crate::decl_func::tests::{named, with_walk};
    use crate::stream::Stream;

    fn rows(src: &str) -> Vec<u8> {
        with_walk(src, |w| {
            let x = named(w, "x", DeclKind::Var)[0];
            let mut s = Stream::new();
            super::module_rows(w, &mut s, x);
            assert_eq!(s.cause(), None);
            s.as_bytes().to_vec()
        })
    }

    // §10.7: ownership not tracked, both rows absent.
    #[test]
    fn a_translation_unit_without_modules_writes_empty_forms() {
        assert_eq!(rows("int x;"), vec![0, 0]);
    }

    // §10.7: an owner, its `UnitInfo` without imports, then the flags.
    #[test]
    fn an_exported_declaration_writes_its_owner() {
        let b = rows("export module m:p;\nexport int x;\n");
        // present, tag PartitionInterface, `m`, `p`, purview|exported,
        // no explicit_defs.
        assert_eq!(b[0], 1);
        assert_eq!(b[1], crate::tables::unit_kind::PARTITION_INTERFACE.tag as u8);
        assert_eq!(*b.last().unwrap(), 0);
        assert_eq!(b[b.len() - 2], 0b11);
    }

    // §10.7: the global module fragment is not the purview.
    #[test]
    fn the_global_module_fragment_is_not_the_purview() {
        let b = rows("module;\nint x;\nexport module m;\n");
        assert_eq!(b[b.len() - 2], 0);
        assert_eq!(b[1], crate::tables::unit_kind::INTERFACE.tag as u8);
    }
}
