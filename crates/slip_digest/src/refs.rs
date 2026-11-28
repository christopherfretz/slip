//! `ref` (`spec/digest.md` §4.1, `spec/digest-kinds.md` §2.2).
//!
//! `crate::tables` has no table of `Ref`: its variant numbers are those of
//! digest §4.1, written here once.

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;

use crate::Cause;
use crate::keys::{KeyRes, Role, info};
use crate::stream::Stream;
use crate::walk::Walk;

/// `Ref::none` (digest §4.1).
pub(crate) const REF_NONE: u64 = 0;
/// `Ref::unit`, then the `KeyId`.
pub(crate) const REF_UNIT: u64 = 1;
/// `Ref::local`, then the ordinal.
pub(crate) const REF_LOCAL: u64 = 2;
/// `Ref::template_param`, then depth and index.
pub(crate) const REF_TEMPLATE_PARAM: u64 = 3;
/// `Ref::param`, then level and index: written by the expression area.
pub(crate) const REF_PARAM: u64 = 4;
/// `Ref::outer`, then the owner's `KeyId` and the index (digest §4.3).
pub(crate) const REF_OUTER: u64 = 5;

impl Walk<'_> {
    /// `ref`: `d` resolved to its entity (through a using-shadow), as
    /// `Ref::template_param`, `Ref::local` or the mapped key. Writing the
    /// key of an implicit entity makes it a derived unit (digest §2.2).
    pub(crate) fn write_ref(&mut self, s: &mut Stream, d: DeclId) {
        // Numbering an owner reads no reference (`local.rs`).
        if crate::local::numbering_mode(self) {
            s.uv(REF_NONE);
            return;
        }
        let ast = self.ast;
        let d = ast.strip_shadow(d);
        match crate::units::failed_substitution(ast, d) {
            crate::units::FailedSubst::Yes => {
                s.uv(REF_NONE).indeterminate(Cause::Unaccounted("ref/failed-substitution"));
                return;
            }
            crate::units::FailedSubst::DepthLimit => {
                s.uv(REF_NONE).indeterminate(Cause::Unaccounted("decl/subst-failed-depth"));
                return;
            }
            crate::units::FailedSubst::No => {}
        }
        let kind = ast.decls.get(d).kind();
        if kind == DeclKind::TemplateTypeParm
            || kind == DeclKind::NonTypeTemplateParm
            || kind == DeclKind::TemplateTemplateParm
        {
            match ast.template_parm(d) {
                Some(p) => {
                    s.uv(REF_TEMPLATE_PARAM).uv(u64::from(p.depth)).uv(u64::from(p.index));
                }
                None => {
                    s.uv(REF_NONE).indeterminate(Cause::Unaccounted("ref/template-param"));
                }
            }
            return;
        }
        let local_object = match info(kind).role {
            Role::Local => true,
            Role::Entity(_) => {
                (kind == DeclKind::Var || kind == DeclKind::Binding)
                    && self.is_local(d)
                    && !self.is_local_extern(d)
            }
            Role::Nothing
            | Role::Transparent
            | Role::Root
            | Role::Content(_)
            | Role::InParent
            | Role::Unsupported(_) => false,
        };
        if local_object {
            if self.unit.expr.profile.is_some() {
                // Inside a profile (digest-kinds §4.4).
                crate::expr_dep::profile_local(self, s, d);
                return;
            }
            if self.in_key > 0 {
                // A key holds no local entity of a unit: a parameter in a
                // signature is `Ref::param`, written by the expression
                // area; anything else here is a shape no rule gives.
                s.uv(REF_NONE).indeterminate(Cause::Unaccounted("key/local-ref"));
                return;
            }
            // Either form depends on the unit (digest §4.2).
            self.unit.local_uses += 1;
            match crate::local::outer(self, d) {
                None => {
                    let n = crate::local::ordinal(self, d);
                    s.uv(REF_LOCAL).uv(n);
                }
                Some(Ok((owner, index, cause))) => {
                    s.uv(REF_OUTER).digest(&owner).uv(index).absorb(cause);
                }
                Some(Err(cause)) => {
                    s.uv(REF_NONE).indeterminate(cause);
                }
            }
            return;
        }
        match self.key(d) {
            KeyRes::Key { id, cause, .. } => {
                s.uv(REF_UNIT).digest(&id).absorb(cause);
                let e = self.entity(d);
                crate::units::demand(self, e);
            }
            KeyRes::None(cause) => {
                s.uv(REF_NONE);
                s.indeterminate(cause.unwrap_or(Cause::Unaccounted("ref/no-key")));
            }
        }
    }

    /// `ref` of an optional entity: `Ref::none` when absent.
    pub(crate) fn write_ref_opt(&mut self, s: &mut Stream, d: Option<DeclId>) {
        match d {
            Some(d) => self.write_ref(s, d),
            None => {
                s.uv(REF_NONE);
            }
        }
    }

    /// `ref*`, in stored order.
    pub(crate) fn write_refs(&mut self, s: &mut Stream, ds: &[DeclId]) {
        s.count(ds.len());
        for &d in ds {
            self.write_ref(s, d);
        }
    }

    /// `ref*sorted`: sorted by encoded bytes, duplicates removed.
    pub(crate) fn write_refs_sorted(&mut self, s: &mut Stream, ds: &[DeclId]) {
        let mut encoded: Vec<Vec<u8>> = Vec::with_capacity(ds.len());
        for &d in ds {
            let mut one = Stream::new();
            self.write_ref(&mut one, d);
            s.absorb(one.cause());
            encoded.push(one.as_bytes().to_vec());
        }
        encoded.sort();
        encoded.dedup();
        s.count(encoded.len());
        for e in &encoded {
            s.raw(e);
        }
    }
}
