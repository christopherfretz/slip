//! The payload records of functions and variables
//! (`spec/digest-kinds.md` §5.4): pending.
//!
//! Each function writes one `record T` for the declaration `d`, read as
//! §2.3 says. Until the package of this file lands, each writes the
//! pending placeholder and then the statements and expressions the
//! record owns, so that the other areas can be reached and tested (the
//! "reach stub": it is no table's encoding and is replaced whole).

use slip_kiln::kiln::ast::DeclId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `FunctionData`.
pub(crate) fn function_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    s.pending(Area::DeclData);
    let body = w.ast.decls.function(d).and_then(|f| f.body);
    s.opt(body.is_some());
    if let Some(b) = body {
        crate::stmt::child(w, s, b);
    }
}

/// `VarData`.
pub(crate) fn var_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    s.pending(Area::DeclData);
    let init = w.ast.decls.var(d).and_then(|v| v.init);
    crate::expr::child_opt(w, s, init);
}

/// `ParamData`.
pub(crate) fn param_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    s.pending(Area::DeclData);
    let default = w.ast.decls.param(d).and_then(|p| p.default_arg);
    crate::expr::child_opt(w, s, default);
}

/// `LetData`.
pub(crate) fn let_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}
