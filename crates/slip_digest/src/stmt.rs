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
#[allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "one arm per kind; `aux`, `a` and `b` are the rows' names in §6"
)]
fn node(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) -> Option<StmtId> {
    crate::walk::stack_probe();
    let n = *w.ast.stmts.get(st);
    let kind = n.kind();
    // kind: enum StmtKind, the node's tag.
    s.tag(stmt_kind::row(&kind));
    // loc: out. (`flags` is gone from Kiln, WP-2026.)
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
            return_stmt(w, s, a, b);
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
        StmtKind::CXXForRangeStmt => Some(range_for(w, s, st)),
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
            coreturn(w, s, aux, a, b);
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

/// `CompoundStmt.aux` (`record FpOverride`: the override in effect, the
/// empty one where no pragma is) and `CompoundStmt.a` (`child*`).
fn compound(w: &mut Walk<'_>, s: &mut Stream, st: StmtId, a: u32) {
    crate::fp::fp_override(s, w.ast.stmts.compound_fp(st));
    let sts: Vec<StmtId> = list(w, s, a, Cause::Unaccounted("StmtNode/CompoundStmt.a"));
    children(w, s, &sts);
}

/// `ReturnStmt.a` (child?) and `ReturnStmt.b` (ref, none when absent).
fn return_stmt(w: &mut Walk<'_>, s: &mut Stream, a: u32, b: u32) {
    expr::child_opt(w, s, ExprId::try_new(a));
    w.write_ref_opt(s, DeclId::try_new(b));
}

/// `CoreturnStmt`: `aux-implicit` (bit 0, the only `bit` row), `a`
/// (child?), `b` (child?, the promise call).
fn coreturn(w: &mut Walk<'_>, s: &mut Stream, aux: u16, a: u32, b: u32) {
    if aux & !1 != 0 {
        s.indeterminate(Cause::Unaccounted("StmtNode/CoreturnStmt.aux"));
    }
    s.flags(FlagWord::new().bit(aux & 1 != 0));
    expr::child_opt(w, s, ExprId::try_new(a));
    expr::child_opt(w, s, ExprId::try_new(b));
}

/// `IfStmt.aux` (`enum IfKind`) and `IfStmt.a` (`record IfData`); `els` is
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

/// `SwitchStmt.a`: `record SwitchData`. `cases` is `out`: the case and
/// default statements, which the walk meets in `body`.
fn switch(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) {
    let &SwitchData { init, cond_var, cond, body, cases: _ } = w.ast.stmts.switch_data(st);
    child_opt(w, s, init);
    decl_opt(w, s, cond_var);
    expr::child(w, s, cond);
    child(w, s, body);
}

/// `WhileStmt.a`, `DoStmt.a`, `ForStmt.a`: `record LoopData`; `body` is
/// the tail. `DoStmt.b` is a location.
fn loop_data(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) -> Option<StmtId> {
    let &LoopData { init, cond_var, cond, inc, body } = w.ast.stmts.loop_data(st);
    child_opt(w, s, init);
    decl_opt(w, s, cond_var);
    expr::child_opt(w, s, cond);
    expr::child_opt(w, s, inc);
    tail_opt(s, body)
}

/// `CXXForRangeStmt.a`: `record RangeForData`; `body` is the tail.
fn range_for(w: &mut Walk<'_>, s: &mut Stream, st: StmtId) -> StmtId {
    let &RangeForData { init, range, begin, end, cond, inc, loop_var, body, colon: _ } =
        w.ast.stmts.range_for_data(st);
    child_opt(w, s, init);
    crate::local::write_decl(w, s, range);
    decl_opt(w, s, begin);
    decl_opt(w, s, end);
    expr::child_opt(w, s, cond);
    expr::child_opt(w, s, inc);
    crate::local::write_decl(w, s, loop_var);
    body
}

/// `CXXExpansionStmt`: `aux-form` (bits 0-1, `enum ExpansionForm`),
/// `aux-expanded` (bit 2, the record's one `bit` row), `b` (uv), `a`
/// (`record ExpansionData`: the pattern parts, then the expansions).
fn expansion(w: &mut Walk<'_>, s: &mut Stream, st: StmtId, aux: u16, b: u32) {
    if aux & !(ExpansionForm::MASK | ExpansionForm::EXPANDED) != 0 {
        s.indeterminate(Cause::Unaccounted("StmtNode/CXXExpansionStmt.aux"));
    }
    // Read from `aux` as `StmtArena::expansion_form` and
    // `expansion_expanded` read it.
    s.tag(expansion_form::row(&ExpansionForm::from_aux(aux)));
    s.flags(FlagWord::new().bit(aux & ExpansionForm::EXPANDED != 0));
    s.uv(u64::from(b));
    let &ExpansionData { init, decl, exprs, range_init, body, prelude, expansions, colon: _ } =
        w.ast.stmts.expansion_data(st);
    child_opt(w, s, init);
    crate::local::write_decl(w, s, decl);
    let es: Vec<ExprId> = list_of(w, s, exprs, Cause::Unaccounted("ExpansionData/exprs"));
    expr::children(w, s, &es);
    expr::child_opt(w, s, range_init);
    child(w, s, body);
    let pre: Vec<StmtId> = list_of(w, s, prelude, Cause::Unaccounted("ExpansionData/prelude"));
    children(w, s, &pre);
    let exp: Vec<StmtId> =
        list_of(w, s, expansions, Cause::Unaccounted("ExpansionData/expansions"));
    children(w, s, &exp);
}

/// `CoroutineBodyStmt.a`: `record CoroutineBodyData`. `promise_stmt` is
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
/// `AsmData`).
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use slip_kiln::kiln::ast::stmt::{ContractKind, ExpansionForm, StmtKind, asm_bits};
    use slip_kiln::kiln::ast::{Ast, StmtId};
    use slip_model::keys::RuleTable;

    use super::{ASM_BITS, child, node};
    use crate::Input;
    use crate::stream::Stream;
    use crate::tables::stmt_kind;
    use crate::walk::{DEPTH_LIMIT, NODE_LIMIT, STACK_LOW, STACK_SIZE, Walk};

    fn parse<R>(src: &str, std: &str, f: impl FnOnce(&slip_kiln::snippet::Parsed<'_>) -> R) -> R {
        let files = [("main.cpp", src)];
        let snippet =
            slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            f(p)
        })
        .expect("parses")
    }

    /// The statements of `ast` of kind `k`, in the order Kiln built them.
    fn of_kind(ast: &Ast, k: StmtKind) -> Vec<StmtId> {
        (1..=ast.stmts.len() as u32)
            .filter_map(StmtId::try_new)
            .filter(|&s| ast.stmts.get(s).kind() == k)
            .collect()
    }

    /// The bytes `g` writes on a fresh walk of `p`.
    fn bytes(
        p: &slip_kiln::snippet::Parsed<'_>,
        g: impl FnOnce(&mut Walk<'_>, &mut Stream),
    ) -> Vec<u8> {
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let rules = RuleTable::empty();
        let mut w = Walk::new(&input, &rules);
        let mut s = Stream::new();
        g(&mut w, &mut s);
        s.as_bytes().to_vec()
    }

    /// The rows of a kind, from the statement and its `(aux, a, b)`.
    type Rows = fn(&mut Walk<'_>, &mut Stream, StmtId, u16, u32, u32);
    /// A change of `(aux, b)`.
    type Change = fn(u16, u32) -> (u16, u32);

    /// The one statement of kind `k` in `src`, and a check that
    /// `node` writes its tag then what `rows` writes for the statement's
    /// own `(aux, a, b)`; then that `rows` writes other bytes for each of
    /// `changed` (a row skipped or written as a constant fails here).
    fn holds(src: &str, std: &str, k: StmtKind, rows: Rows, changed: &[(&str, Change)]) {
        parse(src, std, |p| {
            let st = *of_kind(p.ast, k).first().expect("the statement");
            let n = *p.ast.stmts.get(st);
            let (aux, a, b) = (n.aux(), n.a(), n.b());
            let got = bytes(p, |w, s| {
                node(w, s, st);
            });
            let want = bytes(p, |w, s| {
                s.tag(stmt_kind::row(&k));
                rows(w, s, st, aux, a, b);
            });
            assert_eq!(got, want, "{k:?}: node writes the rows");
            for (what, change) in changed {
                let (aux2, b2) = change(aux, b);
                let other = bytes(p, |w, s| {
                    s.tag(stmt_kind::row(&k));
                    rows(w, s, st, aux2, a, b2);
                });
                assert_ne!(got, other, "{k:?}: {what} is written");
            }
        });
    }

    // GCCAsmStmt.aux-*: the constants are Kiln's `asm_bits`, in row order.
    #[test]
    fn asm_bits_are_kilns() {
        assert_eq!(
            ASM_BITS,
            [asm_bits::VOLATILE, asm_bits::GOTO, asm_bits::INLINE, asm_bits::BASIC]
        );
    }

    // GCCAsmStmt.aux-volatile, -goto, -inline, -basic: each bit is
    // written (the pair test of `aux-goto` changes the labels too).
    #[test]
    fn every_asm_bit_is_written() {
        holds(
            "void f() { asm goto(\"\" : : : : l); l: ; }\n",
            "c++20",
            StmtKind::GCCAsmStmt,
            |w, s, st, aux, _, _| super::asm(w, s, st, aux),
            &[
                ("aux-volatile", |aux, b| (aux ^ asm_bits::VOLATILE, b)),
                ("aux-goto", |aux, b| (aux ^ asm_bits::GOTO, b)),
                ("aux-inline", |aux, b| (aux ^ asm_bits::INLINE, b)),
                ("aux-basic", |aux, b| (aux ^ asm_bits::BASIC, b)),
            ],
        );
    }

    // ReturnStmt.b: the copy-elision candidate is written, alone (no
    // pair of sources changes it without the operand).
    #[test]
    fn the_return_candidate_is_written() {
        holds(
            "struct S { S(); S(const S &); };\nS f() { S s; return s; }\n",
            "c++20",
            StmtKind::ReturnStmt,
            |w, s, _, _, a, b| super::return_stmt(w, s, a, b),
            &[("ReturnStmt.b", |aux, _| (aux, 0))],
        );
        parse("struct S { S(); S(const S &); };\nS f() { S s; return s; }\n", "c++20", |p| {
            let st = of_kind(p.ast, StmtKind::ReturnStmt)[0];
            assert!(p.ast.stmts.return_nrvo(st).is_some(), "Kiln records the candidate");
        });
    }

    // CXXExpansionStmt.aux-expanded and .b: no pair of sources changes
    // either alone (the landed file); each is written.
    #[test]
    fn the_expansion_bit_and_depth_are_written() {
        holds(
            "template <class T> void f() { template for (int e : {1, 2}) ; }\n",
            "c++26",
            StmtKind::CXXExpansionStmt,
            |w, s, st, aux, _, b| super::expansion(w, s, st, aux, b),
            &[
                ("aux-expanded", |aux, b| (aux ^ ExpansionForm::EXPANDED, b)),
                ("aux-form", |aux, b| (aux ^ 1, b)),
                ("b", |aux, b| (aux, b + 1)),
            ],
        );
    }

    // ContractAssertStmt.aux-semantic: Kiln never fills the slot (the
    // landed file), so no source reaches it; it is written.
    #[test]
    fn the_contract_semantic_slot_is_written() {
        holds(
            "void f(int x) { contract_assert(x > 0); }\n",
            "c++26",
            StmtKind::ContractAssertStmt,
            |w, s, _, aux, a, b| super::contract(w, s, aux, a, b),
            &[
                ("aux-semantic 1", |aux, b| (aux | 1 << ContractKind::SEMANTIC_SHIFT, b)),
                ("aux-semantic 2", |aux, b| (aux | 2 << ContractKind::SEMANTIC_SHIFT, b)),
                ("aux-kind", |aux, b| ((aux & !ContractKind::MASK) | 1, b)),
            ],
        );
    }

    // CoreturnStmt.aux-implicit: written (the pair test changes the body
    // too).
    #[test]
    fn the_implicit_coreturn_bit_is_written() {
        let src = "namespace std {\n\
            template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };\n\
            template <class P = void> struct coroutine_handle { static coroutine_handle from_address(void *) noexcept; void *address() const noexcept; };\n\
            struct suspend_never { bool await_ready() const noexcept { return true; } template <class P> void await_suspend(coroutine_handle<P>) const noexcept {} void await_resume() const noexcept {} };\n\
            }\n\
            struct task { struct promise_type { task get_return_object(); std::suspend_never initial_suspend() noexcept; std::suspend_never final_suspend() noexcept; void return_void(); void unhandled_exception(); }; };\n\
            task f() { co_return; }\n";
        holds(
            src,
            "c++20",
            StmtKind::CoreturnStmt,
            |w, s, _, aux, a, b| super::coreturn(w, s, aux, a, b),
            &[("aux-implicit", |aux, b| (aux ^ 1, b))],
        );
    }

    thread_local! {
        static BASE: Cell<usize> = const { Cell::new(0) };
    }

    fn here() -> usize {
        let x = 0u8;
        std::hint::black_box(std::ptr::addr_of!(x) as usize)
    }

    /// The stack bytes the walk of the last statement of `src` (the body
    /// of its last function) takes.
    fn stack_of(src: &str) -> usize {
        parse(src, "c++20", |p| {
            let path = |_| Vec::new();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let rules = RuleTable::empty();
            let mut w = Walk::new(&input, &rules);
            // As `walk::run_with`: owners are numbered before any walk.
            crate::local::number_all(&mut w);
            let body = StmtId::try_new(p.ast.stmts.len() as u32).expect("a statement");
            let mut s = Stream::new();
            STACK_LOW.with(|l| l.set(usize::MAX));
            let base = here();
            child(&mut w, &mut s, body);
            assert_eq!(w.node_depth, 0, "every level left");
            assert_eq!(w.aborts, 0, "below every limit: the frames measured are all walked");
            base.saturating_sub(STACK_LOW.with(Cell::get))
        })
    }

    /// `lib.rs` "What bounds the stack" for statements: measures the bytes
    /// one level of [`NODE_LIMIT`] takes through each statement that
    /// recurses (`child` → `node` → the kind's rows → `child`), and checks
    /// that [`STACK_SIZE`] holds four times `NODE_LIMIT` levels of the
    /// largest. Run in the test profile, whose frames are larger than the
    /// release profile's.
    #[test]
    fn frames_fit_the_stack() {
        const DEEP: usize = 120;
        const FLAT: usize = 20;
        // Each shape: an opening, a closing, and the levels of NODE_LIMIT
        // one nesting takes.
        let shapes: [(&str, &str, &str, usize); 6] = [
            ("block", "{ ", "} ", 1),
            ("if", "if (x) { ", "} ", 2),
            ("else", "if (x) ; else { ", "} ", 1),
            ("switch", "switch (x) { ", "} ", 2),
            ("try", "try { ", "} catch (...) {} ", 2),
            ("range-for", "for (int e : a) { ", "} ", 1),
        ];
        let mut worst = 0;
        for (name, open, close, levels) in shapes {
            let src = |n: usize| {
                format!("void f(int x, int (&a)[2]) {{ {}{} }}\n", open.repeat(n), close.repeat(n))
            };
            let per_level =
                (stack_of(&src(DEEP)) - stack_of(&src(FLAT))) / ((DEEP - FLAT) * levels);
            eprintln!("{name}: {per_level} bytes per level");
            assert!(per_level > 0, "the probes saw the stack");
            worst = worst.max(per_level);
        }
        // Lambdas (WP-040): a closure nested in a lambda's body, and one in
        // a statement expression. Each nesting is a closure's stream and
        // its call operator's, at least one level of DEPTH_LIMIT, so at
        // most DEPTH_LIMIT nestings: measured per nesting, below the limit.
        let lambda_shapes: [(&str, &str, &str); 2] = [
            ("lambda", "[&] { ", "}(); "),
            ("statement-expression lambda", "({ [&] { ", "}(); }); "),
        ];
        let mut worst_lambda = 0;
        for (name, open, close) in lambda_shapes {
            const LDEEP: usize = 40;
            const LFLAT: usize = 10;
            let src = |n: usize| {
                format!("void f(int x, int (&a)[2]) {{ {}{} }}\n", open.repeat(n), close.repeat(n))
            };
            let per_nesting = (stack_of(&src(LDEEP)) - stack_of(&src(LFLAT))) / (LDEEP - LFLAT);
            eprintln!("{name}: {per_nesting} bytes per nesting");
            assert!(per_nesting > 0, "the probes saw the stack");
            worst_lambda = worst_lambda.max(per_nesting);
        }
        let need = NODE_LIMIT as usize * worst + DEPTH_LIMIT as usize * worst_lambda;
        eprintln!(
            "statements: {worst} bytes per level at most; lambdas {worst_lambda} bytes per nesting; \
             NODE_LIMIT {NODE_LIMIT} levels and DEPTH_LIMIT {DEPTH_LIMIT} nestings: {need} bytes; \
             STACK_SIZE {STACK_SIZE}"
        );
        assert!(4 * need <= STACK_SIZE, "STACK_SIZE holds four times {need} bytes");
    }
}
