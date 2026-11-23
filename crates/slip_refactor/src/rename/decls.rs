//! The declarations' names (§9.3, first row): every declaration the AST
//! names like the old name, from `written::decl_names`. A declaration of
//! a subject is a `replace` with a literal piece; one in a file that is
//! not owned sets `read_only` and is not claimed (§9.5); one of any other
//! entity is a `keep` (§4.2).

use slip_kiln::kiln::ast::names::{DeclNameKind, DeclNameView};
use slip_kiln::kiln::ast::{Ast, DeclId, QualType};
use slip_kiln::sites::{Located, Reason};
use slip_model::edits::subject_flags;

use super::{Ctx, Written};

/// The identifier the AST names declaration `d` by, as Kiln's symbol: its
/// own; for a constructor or a destructor its class's; for a deduction
/// guide its template's. `None` for an operator, a conversion function, a
/// literal operator and anything unnamed.
///
/// An inheriting constructor's using-declaration (`using B::B;`) is named
/// by Kiln as a constructor of the class that declares it, so it is given
/// that class's name, not the name written; a decision of the landed file.
pub(crate) fn name_of(ast: &Ast, d: DeclId) -> Option<&[u8]> {
    let name = ast.decls.get(d).name();
    let sym = match name.kind() {
        DeclNameKind::Identifier => name.as_ident()?,
        DeclNameKind::Constructor | DeclNameKind::Destructor => match ast.names.view(name) {
            DeclNameView::Constructor(class) | DeclNameView::Destructor(class) => {
                let class = ast.types.as_record(QualType::unqual(class))?;
                ast.decls.get(class).name().as_ident()?
            }
            _ => return None,
        },
        DeclNameKind::DeductionGuide => match ast.names.view(name) {
            DeclNameView::DeductionGuide(template) => ast.decls.get(template).name().as_ident()?,
            _ => return None,
        },
        DeclNameKind::Operator | DeclNameKind::Conversion | DeclNameKind::LiteralOperator => {
            return None;
        }
    };
    let text = ast.symbols().as_str(sym);
    (!text.is_empty()).then_some(text.as_bytes())
}

/// Claims every declaration named like the old name that no hook settled.
pub(crate) fn claim(ctx: &mut Ctx<'_, '_, '_>) {
    let names = std::mem::take(&mut ctx.named.old);
    for d in &names {
        if ctx.settled.contains(&d.decl) {
            continue;
        }
        let o = super::origin_of(ctx, d.decl);
        let Some(id) = ctx.subjects.of_decl(ctx.keys, o) else {
            ctx.claims.keep(&ctx.sites, d.name);
            continue;
        };
        match ctx.sites.name_token(d.name, ctx.old) {
            // In a file that is not owned: the flag, and no claim.
            Located::Refused { reason: Reason::NotOwned, .. } => {
                ctx.subjects.flag(id, subject_flags::READ_ONLY);
                continue;
            }
            // Refused for another reason first (a macro's body), at a
            // place in a file that is not owned: the flag, and the
            // refusal too.
            Located::Refused { at: Some((f, ..)), .. } if !ctx.sites.is_owned(f) => {
                ctx.subjects.flag(id, subject_flags::READ_ONLY);
            }
            _ => {}
        }
        ctx.claims.replace(&ctx.sites, d.name, &[id], Written::Literal);
    }
    ctx.named.old = names;
}
