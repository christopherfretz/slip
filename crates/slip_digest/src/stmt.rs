//! Statements (`spec/digest-kinds.md` §6, WP-034).
//!
//! A statement is written as the rows of `StmtNode`: its tag, then the
//! rows of its kind in table order, which are its walk order (§2.1). What
//! the tables leave open is `spec/landed/digest-kinds/WP-034.md`.
//!
//! # The walk
//!
//! [`child`] enters one level of `walk::NODE_LIMIT` (`lib.rs` "What
//! bounds the stack") and writes the statement in a loop: a kind whose
//! last row is one statement (the `els` of an `if`, the `sub` of a
//! `case`, the statement of a `default`, a label or an attribute list,
//! the body of a loop or a handler) returns it as its *tail* instead of
//! recursing, and the loop writes it next. Nothing follows a tail in its
//! node, so the bytes are those of a recursive walk, and an
//! `else if` chain, a run of labels (`case 1: case 2: … stmt`), a chain
//! of attributed statements or of nested loops takes one level and one
//! Rust frame. Every other statement child is a [`child`] call: one level
//! each.

use slip_kiln::kiln::ast::stmt::{
    AsmData, AsmOperand, CaseData, CoroutineBodyData, ExpansionData, ExpansionForm, IfData,
    LoopData, RangeForData, StmtKind, SwitchData,
};
use slip_kiln::kiln::ast::{DeclId, ExprId, ListId, RawId, StmtId, StringLitId};

use crate::Cause;
use crate::expr;
use crate::stream::{FlagWord, Stream};
use crate::tables::{contract_kind, expansion_form, if_kind, stmt_kind};
use crate::walk::{TOO_DEEP, Walk};

/// `StmtNode` `flags`, `unsupported` (`unbuilt`): Kiln never sets it.
const FLAGS: Cause = Cause::Unsupported("StmtNode/flags");

/// `child`: the statement `st`, walked here (§6).
pub(crate) fn child(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) {
    if !w.enter_node() {
        s.uv(0).indeterminate(TOO_DEEP);
        return;
    }
    let mut next = Some(st);
    while let Some(cur) = next {
        next = node(w, s, cur);
    }
    w.leave_node();
}

/// `child?` of a statement.
pub(crate) fn child_opt(w: &mut Walk<'_>, s: &mut Stream, st: Option<StmtId>) {
    s.opt(st.is_some());
    if let Some(st) = st {
        child(w, s, st);
    }
}

/// `child*` of statements, in stored order.
fn children(w: &mut Walk<'_>, s: &mut Stream, sts: &[StmtId]) {
    s.count(sts.len());
    for &st in sts {
        child(w, s, st);
    }
}

/// `decl?`.
fn decl_opt(w: &mut Walk<'_>, s: &mut Stream, d: Option<DeclId>) {
    s.opt(d.is_some());
    if let Some(d) = d {
        crate::local::write_decl(w, s, d);
    }
}

/// `child?` of a statement that is the node's last row: the presence
/// byte now, the statement as the tail.
fn tail_opt(s: &mut Stream, st: Option<StmtId>) -> Option<StmtId> {
    s.opt(st.is_some());
    st
}

/// The ids of the list in word `raw`, in stored order; a list word or an
/// element that names nothing is left out with `cause`.
fn list<I: RawId>(w: &Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) -> Vec<I> {
    let ast = w.ast;
    let Some(l) = ListId::try_new(raw).filter(|l| l.index() as usize <= ast.lists.count() + 1)
    else {
        s.indeterminate(cause);
        return Vec::new();
    };
    let words = ast.lists.get(l);
    let mut out = Vec::with_capacity(words.len());
    for &word in words {
        match I::from_raw(word) {
            Some(x) => out.push(x),
            None => {
                s.indeterminate(cause);
            }
        }
    }
    out
}

/// The list `l`, as [`list`] reads a word.
fn list_of<I: RawId>(w: &Walk<'_>, s: &mut Stream, l: ListId, cause: Cause) -> Vec<I> {
    list(w, s, l.raw(), cause)
}

/// The statement a required word names; a placeholder and `cause` when
/// it is 0.
fn required_stmt(s: &mut Stream, raw: u32, cause: Cause) -> Option<StmtId> {
    let st = StmtId::try_new(raw);
    if st.is_none() {
        s.uv(0).indeterminate(cause);
    }
    st
}

/// `child` of the expression a required word names; a placeholder and
/// `cause` when it is 0.
fn required_expr(w: &mut Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) {
    match ExprId::try_new(raw) {
        Some(e) => expr::child(w, s, e),
        None => {
            s.uv(0).indeterminate(cause);
        }
    }
}

/// Writes the rows of `st` and returns its tail, the statement of its
/// last row, which the caller writes next ("The walk" above). Every
/// variant is listed (digest §13 obligation 2).
fn node(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) -> Option<StmtId> {
    crate::walk::stack_probe();
    let n = *w.ast.stmts.get(st);
    let kind = n.kind();
    // kind: enum StmtKind, the node's tag.
    s.tag(stmt_kind::row(&kind));
    // flags: unsupported, unbuilt. loc: out.
    if n.flags() != 0 {
        s.indeterminate(FLAGS);
    }
    let (aux, a, b) = (n.aux(), n.a(), n.b());
    match kind {
        StmtKind::CompoundStmt => {
            compound(w, s, st, a);
            None
        }
        StmtKind::DeclStmt => {
            // DeclStmt.a: decl*.
            let ds: Vec<DeclId> = list(w, s, a, Cause::Unaccounted("StmtNode/DeclStmt.a"));
            s.count(ds.len());
            for d in ds {
                crate::local::write_decl(w, s, d);
            }
            None
        }
        StmtKind::ExprStmt => {
            required_expr(w, s, a, Cause::Unaccounted("StmtNode/ExprStmt.a"));
            None
        }
        StmtKind::NullStmt | StmtKind::BreakStmt | StmtKind::ContinueStmt => None,
        StmtKind::IfStmt => if_stmt(w, s, st, aux),
        StmtKind::SwitchStmt => {
            switch(w, s, st);
            None
        }
        StmtKind::CaseStmt => {
            // CaseStmt.a: record CaseData; `sub` is the tail.
            let &CaseData { lhs, rhs, sub } = w.ast.stmts.case_data(st);
            expr::child(w, s, lhs);
            expr::child_opt(w, s, rhs);
            Some(sub)
        }
        StmtKind::DefaultStmt => required_stmt(s, a, Cause::Unaccounted("StmtNode/DefaultStmt.a")),
        StmtKind::WhileStmt | StmtKind::DoStmt | StmtKind::ForStmt => loop_data(w, s, st),
        StmtKind::ReturnStmt => {
            // ReturnStmt.a: child?; ReturnStmt.b: ref (none when absent).
            expr::child_opt(w, s, ExprId::try_new(a));
            w.write_ref_opt(s, DeclId::try_new(b));
            None
        }
        StmtKind::LabelStmt => {
            // LabelStmt.a: decl, the label; LabelStmt.b: the tail.
            match DeclId::try_new(a) {
                Some(d) => crate::local::write_decl(w, s, d),
                None => {
                    s.uv(0).indeterminate(Cause::Unaccounted("StmtNode/LabelStmt.a"));
                }
            }
            required_stmt(s, b, Cause::Unaccounted("StmtNode/LabelStmt.b"))
        }
        StmtKind::GotoStmt => {
            match DeclId::try_new(a) {
                Some(d) => w.write_ref(s, d),
                None => {
                    s.uv(0).indeterminate(Cause::Unaccounted("StmtNode/GotoStmt.a"));
                }
            }
            None
        }
        StmtKind::IndirectGotoStmt => {
            required_expr(w, s, a, Cause::Unaccounted("StmtNode/IndirectGotoStmt.a"));
            None
        }
        StmtKind::AttributedStmt => {
            // AttributedStmt.a: attrs; AttributedStmt.b: the tail.
            match ListId::try_new(a) {
                Some(l) => crate::attrs::stmt_attrs(w, s, l),
                None => {
                    s.uv(0).indeterminate(Cause::Unaccounted("StmtNode/AttributedStmt.a"));
                }
            }
            required_stmt(s, b, Cause::Unaccounted("StmtNode/AttributedStmt.b"))
        }
        StmtKind::CXXForRangeStmt => range_for(w, s, st),
        StmtKind::CXXExpansionStmt => {
            expansion(w, s, st, aux, b);
            None
        }
        StmtKind::CXXTryStmt => {
            // CXXTryStmt.a: child; CXXTryStmt.b: child*, the handlers.
            if let Some(block) = required_stmt(s, a, Cause::Unaccounted("StmtNode/CXXTryStmt.a")) {
                child(w, s, block);
            }
            let handlers: Vec<StmtId> = list(w, s, b, Cause::Unaccounted("StmtNode/CXXTryStmt.b"));
            children(w, s, &handlers);
            None
        }
        StmtKind::CXXCatchStmt => {
            // CXXCatchStmt.a: decl?; CXXCatchStmt.b: the tail.
            decl_opt(w, s, DeclId::try_new(a));
            required_stmt(s, b, Cause::Unaccounted("StmtNode/CXXCatchStmt.b"))
        }
        StmtKind::CoroutineBodyStmt => {
            coroutine_body(w, s, st);
            None
        }
        StmtKind::CoreturnStmt => {
            // aux-implicit: bit 0, the only `bit` row; a: child?; b: child?.
            if aux & !1 != 0 {
                s.indeterminate(Cause::Unaccounted("StmtNode/CoreturnStmt.aux"));
            }
            s.flags(FlagWord::new().bit(aux & 1 != 0));
            expr::child_opt(w, s, ExprId::try_new(a));
            expr::child_opt(w, s, ExprId::try_new(b));
            None
        }
        StmtKind::GCCAsmStmt => {
            asm(w, s, st, aux);
            None
        }
        StmtKind::ContractAssertStmt => {
            contract(w, s, aux, a, b);
            None
        }
    }
}

/// `CompoundStmt.aux` (record FpOverride: the override in effect, the
/// empty one where no pragma is) and `CompoundStmt.a` (`child*`).
fn compound(w: &mut Walk<'_>, s: &mut Stream, st: StmtId, a: u32) {
    crate::fp::fp_override(s, w.ast.stmts.compound_fp(st));
    let sts: Vec<StmtId> = list(w, s, a, Cause::Unaccounted("StmtNode/CompoundStmt.a"));
    children(w, s, &sts);
}

/// `IfStmt.aux` (enum IfKind) and `IfStmt.a` (record IfData); `els` is
/// the tail.
fn if_stmt(w: &mut Walk<'_>, s: &mut Stream, st: StmtId, aux: u16) -> Option<StmtId> {
    if aux > 3 {
        s.indeterminate(Cause::Unaccounted("StmtNode/IfStmt.aux"));
    }
    s.tag(if_kind::row(&w.ast.stmts.if_kind(st)));
    let &IfData { init, cond_var, cond, then, els } = w.ast.stmts.if_data(st);
    child_opt(w, s, init);
    decl_opt(w, s, cond_var);
    expr::child_opt(w, s, cond);
    child_opt(w, s, then);
    tail_opt(s, els)
}

/// `SwitchStmt.a`: record SwitchData. `cases` is `out`: the case and
/// default statements, which the walk meets in `body`.
fn switch(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) {
    let &SwitchData { init, cond_var, cond, body, cases: _ } = w.ast.stmts.switch_data(st);
    child_opt(w, s, init);
    decl_opt(w, s, cond_var);
    expr::child(w, s, cond);
    child(w, s, body);
}

/// `WhileStmt.a`, `DoStmt.a`, `ForStmt.a`: record LoopData; `body` is
/// the tail. `DoStmt.b` is a location.
fn loop_data(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) -> Option<StmtId> {
    let &LoopData { init, cond_var, cond, inc, body } = w.ast.stmts.loop_data(st);
    child_opt(w, s, init);
    decl_opt(w, s, cond_var);
    expr::child_opt(w, s, cond);
    expr::child_opt(w, s, inc);
    tail_opt(s, body)
}

/// `CXXForRangeStmt.a`: record RangeForData; `body` is the tail.
fn range_for(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) -> Option<StmtId> {
    let &RangeForData { init, range, begin, end, cond, inc, loop_var, body, colon: _ } =
        w.ast.stmts.range_for_data(st);
    child_opt(w, s, init);
    crate::local::write_decl(w, s, range);
    decl_opt(w, s, begin);
    decl_opt(w, s, end);
    expr::child_opt(w, s, cond);
    expr::child_opt(w, s, inc);
    crate::local::write_decl(w, s, loop_var);
    Some(body)
}

/// `CXXExpansionStmt`: `aux-form` (bits 0-1, enum ExpansionForm),
/// `aux-expanded` (bit 2, the record's one `bit` row), `b` (uv), `a`
/// (record ExpansionData: the pattern parts, then the expansions).
fn expansion(w: &mut Walk<'_>, s: &mut Stream, st: StmtId, aux: u16, b: u32) {
    if aux & !(ExpansionForm::MASK | ExpansionForm::EXPANDED) != 0 {
        s.indeterminate(Cause::Unaccounted("StmtNode/CXXExpansionStmt.aux"));
    }
    s.tag(expansion_form::row(&w.ast.stmts.expansion_form(st)));
    s.flags(FlagWord::new().bit(w.ast.stmts.expansion_expanded(st)));
    s.uv(u64::from(b));
    let &ExpansionData { init, decl, exprs, range_init, body, prelude, expansions, colon: _ } =
        w.ast.stmts.expansion_data(st);
    child_opt(w, s, init);
    crate::local::write_decl(w, s, decl);
    let es: Vec<ExprId> =
        list_of(w, s, exprs, Cause::Unaccounted("ExpansionData/exprs"));
    expr::children(w, s, &es);
    expr::child_opt(w, s, range_init);
    child(w, s, body);
    let pre: Vec<StmtId> = list_of(w, s, prelude, Cause::Unaccounted("ExpansionData/prelude"));
    children(w, s, &pre);
    let exp: Vec<StmtId> =
        list_of(w, s, expansions, Cause::Unaccounted("ExpansionData/expansions"));
    children(w, s, &exp);
}

/// `CoroutineBodyStmt.a`: record CoroutineBodyData. `promise_stmt` is
/// `out`: the `DeclStmt` of `promise`, which that row writes.
fn coroutine_body(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) {
    let &CoroutineBodyData {
        body,
        promise,
        promise_stmt: _,
        init_suspend,
        final_suspend,
        on_exception,
        on_fallthrough,
        allocate,
        deallocate,
        return_value,
        result_decl,
        return_stmt,
        return_on_alloc_failure,
        param_moves,
    } = w.ast.stmts.coroutine_body(st);
    child(w, s, body);
    crate::local::write_decl(w, s, promise);
    expr::child(w, s, init_suspend);
    expr::child(w, s, final_suspend);
    child_opt(w, s, on_exception);
    child_opt(w, s, on_fallthrough);
    expr::child(w, s, allocate);
    expr::child(w, s, deallocate);
    expr::child(w, s, return_value);
    decl_opt(w, s, result_decl);
    child_opt(w, s, return_stmt);
    child_opt(w, s, return_on_alloc_failure);
    let moves: Vec<StmtId> =
        list_of(w, s, param_moves, Cause::Unaccounted("CoroutineBodyData/param_moves"));
    children(w, s, &moves);
}

/// The `aux` bits of a `GCCAsmStmt`, in the order of their rows
/// (`aux-volatile`, `aux-goto`, `aux-inline`, `aux-basic`): Kiln's
/// `asm_bits` (held equal by a unit test).
const ASM_BITS: [u16; 4] = [1, 2, 4, 8];

/// `GCCAsmStmt`: the four `aux` bits as one flag word, then `a` (record
/// AsmData).
fn asm(w: &mut Walk<'_>, s: &mut Stream, st: StmtId, aux: u16) {
    let known = ASM_BITS.iter().fold(0, |m, b| m | b);
    if aux & !known != 0 {
        s.indeterminate(Cause::Unaccounted("StmtNode/GCCAsmStmt.aux"));
    }
    let mut word = FlagWord::new();
    for bit in ASM_BITS {
        word = word.bit(aux & bit != 0);
    }
    s.flags(word);
    let ast = w.ast;
    let &AsmData {
        string,
        string_loc: _,
        operands: _,
        num_outputs,
        num_inputs,
        clobbers,
        labels,
        rparen: _,
        srclocs: _,
    } = ast.stmts.asm_data(st);
    crate::value::string(w, s, string);
    // operands: record AsmOperand*, outputs then inputs; Kiln's slice is
    // `num_outputs + num_inputs` long (`StmtArena::asm_operands`).
    let ops: Vec<AsmOperand> = ast.stmts.asm_operands(st).to_vec();
    if u64::from(num_outputs) + u64::from(num_inputs) != ops.len() as u64 {
        s.indeterminate(Cause::Unaccounted("AsmData/num_inputs"));
    }
    s.count(ops.len());
    for AsmOperand { name, constraint, constraint_loc: _, expr: e } in ops {
        s.opt(name.is_some());
        if let Some(name) = name {
            w.write_sym(s, name);
        }
        crate::value::string(w, s, constraint);
        expr::child(w, s, e);
    }
    s.uv(u64::from(num_outputs));
    let cl: Vec<StringLitId> = list_of(w, s, clobbers, Cause::Unaccounted("AsmData/clobbers"));
    s.count(cl.len());
    for l in cl {
        crate::value::string(w, s, l);
    }
    let ls: Vec<DeclId> = list_of(w, s, labels, Cause::Unaccounted("AsmData/labels"));
    w.write_refs(s, &ls);
}

/// `ContractAssertStmt`: `aux-kind` (bits 0-1), `aux-semantic` (bits
/// 2-3, uv), `b` (decl?), `a` (child). The rows of its `ContractSite`
/// are symbolic and write nothing (§1.2, digest §9).
fn contract(w: &mut Walk<'_>, s: &mut Stream, aux: u16, a: u32, b: u32) {
    use slip_kiln::kiln::ast::stmt::ContractKind;
    if aux >> (ContractKind::SEMANTIC_SHIFT + 2) != 0 || aux & ContractKind::MASK == 3 {
        s.indeterminate(Cause::Unaccounted("StmtNode/ContractAssertStmt.aux"));
    }
    s.tag(contract_kind::row(&ContractKind::from_aux(aux)));
    s.uv(u64::from((aux >> ContractKind::SEMANTIC_SHIFT) & 3));
    decl_opt(w, s, DeclId::try_new(b));
    required_expr(w, s, a, Cause::Unaccounted("StmtNode/ContractAssertStmt.a"));
}

