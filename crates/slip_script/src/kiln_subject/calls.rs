//! Calls and the extents of expressions (`spec/api.md` §6.6 rules 3 and
//! 4, §6.7, §7.4 rule 3; `docs/wp/WP-086.md`,
//! `spec/landed/api/WP-086.md`).
//!
//! **The index.** On the first question about a call, one walk of the
//! bodies and initialisers of every written declaration records each call
//! of §6.7 rule 1 ([`Index`]). The walk starts at the translation unit,
//! goes down member lists (never into an implicit declaration or an
//! instantiation: rule 2), and keeps its own stack, so no depth of nesting
//! in the source deepens the thread's stack.
//!
//! **Numbering** (the contract of `subject.rs`): an expression is numbered
//! by its `ExprId` index. The live expressions are the calls and the
//! arguments of calls. A reference is numbered by its place in the
//! written-reference list the subject read when it was made, plus one
//! ([`KilnSubject::reference_live`]).
//!
//! **Extents are exact or null** (§6.6 rule 4). The first and the last
//! token of an expression are derived only for the kinds of the table in
//! `spec/landed/api/WP-086.md`, each from a location Kiln stores for that
//! token; every other kind, and every expression that has one of them where
//! the rule descends, has none. The range of the two tokens is
//! `slip_kiln::ranges`' (WP-084): this module computes no range and no end
//! of a token of its own.

use std::collections::HashMap;

use slip_kiln::kiln::ast::decl::{DeclKind, Defaulted, FriendTarget};
use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::names::OverloadedOperator as Op;
use slip_kiln::kiln::ast::stmt::StmtKind;
use slip_kiln::kiln::ast::templates::Requirement;
use slip_kiln::kiln::ast::types::{Nns, TypeKind};
use slip_kiln::kiln::ast::{DeclId, ExprId, StmtId};
use slip_kiln::kiln::source::{Loc, Span};
use slip_kiln::ranges::{self, Tier};
use slip_kiln::written::{Role, WrittenRef};

use super::KilnSubject;
use super::entities::function_kind;
use crate::subject::{Expression, File, PlaceOf, Range, Reference};

/// One call of §6.7 rule 1.
#[derive(Debug)]
struct Call {
    expr: ExprId,
    /// The function called, as resolved (the callee's declaration).
    callee: DeclId,
    /// Index of the callee's reference in `KilnSubject::refs`.
    reference: u32,
    /// The arguments as written (rule 5).
    args: Vec<ExprId>,
}

/// The calls of the unit, built once ([`KilnSubject::index`]).
#[derive(Debug, Default)]
pub(super) struct Index {
    /// Every call, in unit order.
    calls: Vec<Call>,
    /// Expression index → index in `calls`.
    by_expr: HashMap<u32, u32>,
    /// Origin of the callee (a declaration index) → indexes in `calls`, in
    /// unit order.
    by_origin: HashMap<u32, Vec<u32>>,
    /// Expression index of every live expression (a call or an argument)
    /// → the declaration that holds it.
    holder: HashMap<u32, DeclId>,
    /// Expressions of rule 1's shape whose callee's name has no entry in
    /// the written-reference table: not calls (`docs/wp/WP-086.md` item
    /// 2). The counter `api.calls_without_reference`.
    without_reference: u64,
}

/// Why an expression has no extent: the kind of the expression where the
/// rule stopped, and what was missing there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoExtent {
    /// The expression kind.
    pub kind: ExprKind,
    /// What the rule could not establish.
    pub reason: &'static str,
}

impl NoExtent {
    fn new(kind: ExprKind, reason: &'static str) -> NoExtent {
        NoExtent { kind, reason }
    }
}

impl PartialOrd for NoExtent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NoExtent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        ((self.kind as u8), self.reason).cmp(&((other.kind as u8), other.reason))
    }
}

/// A call and where its `)` is: the subject path and byte offset
/// ([`KilnSubject::call_sites`]).
pub type CallSite<'a> = (Expression, Option<(&'a [u8], u32)>);

/// The census of extents over the live expressions of a subject
/// ([`KilnSubject::extent_census`]): for tests, and for the hand-back's
/// counts on real units.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtentCensus {
    /// How many calls.
    pub calls: u64,
    /// Calls with no extent, by cause.
    pub calls_null: std::collections::BTreeMap<NoExtent, u64>,
    /// How many arguments (each argument of each call).
    pub arguments: u64,
    /// Arguments with no extent, by cause.
    pub arguments_null: std::collections::BTreeMap<NoExtent, u64>,
    /// Expressions of rule 1's shape not made calls: no table entry.
    pub without_reference: u64,
}

/// One item of the walk's stack: a node and the declaration that holds it
/// (`None` above every unit: a namespace's members are their own holders).
#[derive(Clone, Copy)]
enum Item {
    Decl(DeclId, Option<DeclId>),
    Stmt(StmtId, DeclId),
    Expr(ExprId, DeclId),
}

/// A set of arena indexes.
struct Bits(Vec<u64>);

impl Bits {
    fn new(len: usize) -> Bits {
        Bits(vec![0; len / 64 + 2])
    }

    /// Inserts `i`; whether it was absent.
    fn insert(&mut self, i: u32) -> bool {
        let (w, b) = (i as usize / 64, 1u64 << (i % 64));
        if w >= self.0.len() {
            self.0.resize(w + 1, 0);
        }
        let fresh = self.0[w] & b == 0;
        self.0[w] |= b;
        fresh
    }
}

impl KilnSubject<'_> {
    /// The calls of the unit, built on first use.
    fn call_index(&self) -> &Index {
        self.calls.get_or_init(|| self.build_call_index())
    }

    // ---- the walk ------------------------------------------------------------

    fn build_call_index(&self) -> Index {
        let ast = self.ast;
        let mut seen_decl = Bits::new(ast.decls.len());
        let mut seen_expr = Bits::new(ast.exprs.len());
        let mut seen_stmt = Bits::new(ast.stmts.len());
        let mut stack = vec![Item::Decl(ast.tu, None)];
        let mut found: Vec<(Call, DeclId)> = Vec::new();
        let mut without_reference = 0u64;
        while let Some(item) = stack.pop() {
            match item {
                Item::Decl(d, holder) => {
                    if !seen_decl.insert(d.index()) {
                        continue;
                    }
                    if d != ast.tu && self.instantiated_or_implicit(d) {
                        continue;
                    }
                    self.push_decl(d, holder, &mut stack);
                }
                Item::Stmt(s, holder) => {
                    if seen_stmt.insert(s.index()) {
                        self.push_stmt(s, holder, &mut stack);
                    }
                }
                Item::Expr(e, holder) => {
                    if !seen_expr.insert(e.index()) {
                        continue;
                    }
                    match self.call_of(e) {
                        Ok(Some(c)) => found.push((c, holder)),
                        Ok(None) => {}
                        Err(()) => without_reference += 1,
                    }
                    self.push_expr(e, holder, &mut stack);
                }
            }
        }
        // Unit order: the first token, then the callee's name, then the
        // arena (two calls at one place: none on a well-formed AST).
        let key = |c: &Call| {
            let first = self.first_token(c.expr).unwrap_or_else(|_| ast.begin_loc(c.expr));
            (first, self.refs[c.reference as usize].name.lo)
        };
        let mut keyed: Vec<((Loc, Loc), Call, DeclId)> =
            found.into_iter().map(|(c, h)| (key(&c), c, h)).collect();
        keyed.sort_by(|(ka, a, _), (kb, b, _)| {
            self.order(ka.0, kb.0).then_with(|| self.order(ka.1, kb.1)).then(a.expr.cmp(&b.expr))
        });
        let mut index = Index { without_reference, ..Index::default() };
        for (_, c, h) in keyed {
            let i = index.calls.len() as u32;
            index.by_expr.insert(c.expr.index(), i);
            let origin = self.origin_decl(self.entity(c.callee));
            index.by_origin.entry(origin.index()).or_default().push(i);
            index.holder.insert(c.expr.index(), h);
            for a in &c.args {
                index.holder.entry(a.index()).or_insert(h);
            }
            index.calls.push(c);
        }
        index
    }

    /// The parts of declaration `d` that hold expressions, onto the stack.
    fn push_decl(&self, d: DeclId, holder: Option<DeclId>, stack: &mut Vec<Item>) {
        let ast = self.ast;
        let decls = &ast.decls;
        let kind = self.kind(d);
        let h = holder.unwrap_or_else(|| self.as_written(d));
        let expr = |e: Option<ExprId>, stack: &mut Vec<Item>| {
            if let Some(e) = e {
                stack.push(Item::Expr(e, h));
            }
        };
        if let Some(t) = ast.template_info(d) {
            expr(ast.templates.param_list(t.params).requires, stack);
            // A concept's definition is its constraint expression.
            if kind == DeclKind::Concept {
                expr(t.constraint, stack);
            }
            if let Some(p) = t.pattern {
                stack.push(Item::Decl(p, holder));
            }
        }
        if kind == DeclKind::Friend {
            match decls.friend_target(d) {
                FriendTarget::Decl(x) | FriendTarget::Type { owned_tag: Some(x), .. } => {
                    stack.push(Item::Decl(x, holder));
                }
                FriendTarget::Type { owned_tag: None, .. } => {}
            }
        }
        if function_kind(kind) {
            self.push_function(d, h, stack);
        } else if let Some(v) = decls.var(d) {
            expr(v.init, stack);
        } else if let Some(f) = decls.field(d) {
            expr(f.bit_width, stack);
            expr(f.init, stack);
        } else if let Some(x) = decls.enumerator(d) {
            expr(x.init, stack);
        } else if let Some(s) = decls.static_assert(d) {
            expr(Some(s.cond), stack);
            expr(s.message, stack);
        } else if decls.member_list(d).is_some() {
            // A namespace, a class, an enumeration, a linkage
            // specification: each member holds its own (a member of a
            // local class is held by the holder of the class).
            let members: Vec<DeclId> = decls.members(d).collect();
            for m in members.into_iter().rev() {
                stack.push(Item::Decl(m, holder));
            }
        }
    }

    /// A function's default arguments, member initialisers as written,
    /// constraints, contracts and body (not the body of a defaulted
    /// function: Kiln makes it).
    fn push_function(&self, d: DeclId, h: DeclId, stack: &mut Vec<Item>) {
        let ast = self.ast;
        let Some(f) = ast.decls.function(d) else { return };
        let mut items = Vec::new();
        for p in ast.lists.iter::<DeclId>(f.params) {
            if let Some(e) = ast.decls.param(p).and_then(|p| p.default_arg) {
                items.push(Item::Expr(e, h));
            }
        }
        if let Some(&r) = ast.templates.trailing_requires.get(d) {
            items.push(Item::Expr(r, h));
        }
        if let Some(&c) = ast.decls.cold.contracts.get(d) {
            items.extend(ast.lists.iter::<StmtId>(c).map(|s| Item::Stmt(s, h)));
        }
        items.extend(ast.ctor_inits(d).filter(|i| i.is_written()).map(|i| Item::Expr(i.init, h)));
        if f.spec.defaulted() == Defaulted::No
            && let Some(b) = f.body
        {
            items.push(Item::Stmt(b, h));
        }
        stack.extend(items.into_iter().rev());
    }

    /// The parts of statement `s` that are written.
    fn push_stmt(&self, s: StmtId, h: DeclId, stack: &mut Vec<Item>) {
        let ast = self.ast;
        let stmts = &ast.stmts;
        let lists = &ast.lists;
        let mut items: Vec<Item> = Vec::new();
        let st =
            |s: Option<StmtId>, items: &mut Vec<Item>| items.extend(s.map(|s| Item::Stmt(s, h)));
        let ex =
            |e: Option<ExprId>, items: &mut Vec<Item>| items.extend(e.map(|e| Item::Expr(e, h)));
        let var = |d: Option<DeclId>, items: &mut Vec<Item>| {
            items.extend(d.map(|d| Item::Decl(d, Some(h))));
        };
        match stmts.get(s).kind() {
            StmtKind::CompoundStmt => {
                items
                    .extend(lists.iter::<StmtId>(stmts.compound_body(s)).map(|c| Item::Stmt(c, h)));
            }
            StmtKind::DeclStmt => {
                items.extend(
                    lists.iter::<DeclId>(stmts.decl_stmt_decls(s)).map(|d| Item::Decl(d, Some(h))),
                );
            }
            StmtKind::ExprStmt => ex(Some(stmts.expr_stmt_expr(s)), &mut items),
            StmtKind::IfStmt => {
                let i = *stmts.if_data(s);
                st(i.init, &mut items);
                var(i.cond_var, &mut items);
                ex(i.cond, &mut items);
                st(i.then, &mut items);
                st(i.els, &mut items);
            }
            StmtKind::SwitchStmt => {
                let w = *stmts.switch_data(s);
                st(w.init, &mut items);
                var(w.cond_var, &mut items);
                ex(Some(w.cond), &mut items);
                st(Some(w.body), &mut items);
            }
            StmtKind::CaseStmt => {
                let c = *stmts.case_data(s);
                ex(Some(c.lhs), &mut items);
                ex(c.rhs, &mut items);
                st(Some(c.sub), &mut items);
            }
            StmtKind::DefaultStmt => st(Some(stmts.default_sub(s)), &mut items),
            StmtKind::LabelStmt | StmtKind::AttributedStmt => {
                st(Some(stmts.sub_stmt(s)), &mut items);
            }
            StmtKind::WhileStmt | StmtKind::ForStmt | StmtKind::DoStmt => {
                let l = *stmts.loop_data(s);
                st(l.init, &mut items);
                var(l.cond_var, &mut items);
                ex(l.cond, &mut items);
                ex(l.inc, &mut items);
                st(l.body, &mut items);
            }
            // The written parts only: the range's variable is Kiln's, its
            // initialiser is the range as written; the iterators, the
            // condition, the increment and the loop variable's initialiser
            // are Kiln's.
            StmtKind::CXXForRangeStmt => {
                let r = *stmts.range_for_data(s);
                st(r.init, &mut items);
                ex(ast.decls.var(r.range).and_then(|v| v.init), &mut items);
                st(Some(r.body), &mut items);
            }
            // The pattern as written; never the expansions, which are its
            // instantiations (§6.7 rule 2).
            StmtKind::CXXExpansionStmt => {
                let x = *stmts.expansion_data(s);
                st(x.init, &mut items);
                items.extend(lists.iter::<ExprId>(x.exprs).map(|e| Item::Expr(e, h)));
                ex(x.range_init, &mut items);
                st(Some(x.body), &mut items);
            }
            StmtKind::ReturnStmt => ex(stmts.return_value(s), &mut items),
            StmtKind::IndirectGotoStmt => ex(Some(stmts.indirect_goto_target(s)), &mut items),
            StmtKind::ContractAssertStmt => ex(Some(stmts.contract_cond(s)), &mut items),
            // The operand; the promise's call is Kiln's.
            StmtKind::CoreturnStmt => ex(stmts.coreturn_operand(s), &mut items),
            // The body as written; the rest is Kiln's.
            StmtKind::CoroutineBodyStmt => st(Some(stmts.coroutine_body(s).body), &mut items),
            StmtKind::CXXTryStmt => {
                st(Some(stmts.try_block(s)), &mut items);
                items.extend(lists.iter::<StmtId>(stmts.try_handlers(s)).map(|c| Item::Stmt(c, h)));
            }
            StmtKind::CXXCatchStmt => st(Some(stmts.catch_block(s)), &mut items),
            StmtKind::GCCAsmStmt => {
                items.extend(stmts.asm_operands(s).iter().map(|o| Item::Expr(o.expr, h)));
            }
            StmtKind::NullStmt
            | StmtKind::BreakStmt
            | StmtKind::ContinueStmt
            | StmtKind::GotoStmt => {}
        }
        stack.extend(items.into_iter().rev());
    }

    /// The subexpressions of `e` that are written: Kiln's children, and
    /// what an expression holds outside them (a lambda's body, a statement
    /// expression's statements, a requires-expression's requirements); not
    /// the calls Kiln makes for a `co_await`.
    fn push_expr(&self, e: ExprId, h: DeclId, stack: &mut Vec<Item>) {
        let ast = self.ast;
        let exprs = &ast.exprs;
        let n = exprs.get(e);
        let mut items: Vec<Item> = Vec::new();
        match n.kind() {
            ExprKind::LambdaExpr => {
                let l = exprs.lambda(e);
                items.extend(exprs.children(e, &ast.lists).into_iter().map(|c| Item::Expr(c, h)));
                if let Some(op) = l.call_op {
                    let op = ast.template_info(op).and_then(|t| t.pattern).unwrap_or(op);
                    if let Some(f) = ast.decls.function(op) {
                        for p in ast.lists.iter::<DeclId>(f.params) {
                            if let Some(d) = ast.decls.param(p).and_then(|p| p.default_arg) {
                                items.push(Item::Expr(d, h));
                            }
                        }
                    }
                    if let Some(&c) = ast.decls.cold.contracts.get(op) {
                        items.extend(ast.lists.iter::<StmtId>(c).map(|s| Item::Stmt(s, h)));
                    }
                }
                items.extend(l.body.map(|b| Item::Stmt(b, h)));
            }
            ExprKind::StmtExpr => items.push(Item::Stmt(exprs.stmt_expr_body(e), h)),
            ExprKind::RequiresExpr => {
                if let Some(r) = ast.requires_of(e) {
                    for q in ast.templates.requirements(r) {
                        match *q {
                            Requirement::Simple(x)
                            | Requirement::Nested(x)
                            | Requirement::Compound { expr: x, .. } => items.push(Item::Expr(x, h)),
                            Requirement::Type(..) => {}
                        }
                    }
                }
            }
            ExprKind::CoawaitExpr | ExprKind::CoyieldExpr => {
                items.push(Item::Expr(exprs.coawait_operand(e, &ast.lists), h));
            }
            _ => items.extend(exprs.children(e, &ast.lists).into_iter().map(|c| Item::Expr(c, h))),
        }
        stack.extend(items.into_iter().rev());
    }

    // ---- what a call is (§6.7 rules 1, 2, 5) ---------------------------------

    /// The call `e` is, `Ok(None)` when it is none, `Err(())` when it has
    /// rule 1's shape but its callee's name is in no table entry.
    fn call_of(&self, e: ExprId) -> Result<Option<Call>, ()> {
        let ast = self.ast;
        let exprs = &ast.exprs;
        let n = exprs.get(e);
        // Rule 1: a written postfix call (`postfix`).
        let callee = match n.kind() {
            ExprKind::CallExpr | ExprKind::CXXMemberCallExpr => self.strip_parens(exprs.callee(e)),
            _ => return Ok(None),
        };
        let c = exprs.get(callee);
        if !matches!(c.kind(), ExprKind::DeclRefExpr | ExprKind::MemberExpr)
            || !self.postfix(e, callee)
        {
            return Ok(None);
        }
        let decl = exprs.decl(callee);
        if !function_kind(self.kind(decl)) {
            return Ok(None);
        }
        let Some(reference) = self.reference_index(callee) else { return Err(()) };
        let mut args: Vec<ExprId> = ast
            .lists
            .iter::<ExprId>(exprs.call_args(e))
            .filter(|&a| exprs.get(a).kind() != ExprKind::CXXDefaultArgExpr)
            .map(|a| self.as_written_expr(a))
            .collect();
        // `s.f(2)` with an explicit object parameter: Kiln's first argument
        // is the object (rule 5: not an argument).
        if self.explicit_object(e) && !args.is_empty() {
            args.remove(0);
        }
        Ok(Some(Call { expr: e, callee: decl, reference, args }))
    }

    /// Whether call `e` with callee `callee` (without its parentheses)
    /// is written `callee ( arguments )`: Kiln stores its `)`, after the
    /// callee's location. A call Kiln makes has none (a conversion
    /// function's), or one at the callee (the same); a user-defined
    /// literal's has the literal, before the callee (its suffix).
    fn postfix(&self, e: ExprId, callee: ExprId) -> bool {
        let exprs = &self.ast.exprs;
        exprs.end_loc(e).is_some_and(|close| self.order(exprs.get(callee).loc(), close).is_lt())
    }

    /// The table entries whose name starts at `at`.
    fn refs_named_at(&self, at: Loc) -> impl Iterator<Item = (u32, &WrittenRef)> + '_ {
        let names = self.ref_names.get_or_init(|| {
            let mut m: HashMap<u32, Vec<u32>> = HashMap::new();
            for (i, r) in self.refs.iter().enumerate() {
                m.entry(r.name.lo.to_raw()).or_default().push(i as u32);
            }
            m
        });
        names.get(&at.to_raw()).into_iter().flatten().map(|&i| (i, &self.refs[i as usize]))
    }

    /// The table entry of the name of `x` (a `DeclRefExpr` or a
    /// `MemberExpr`), as an index in `refs`: the entry whose name starts
    /// at the expression's location, in the role of an expression or
    /// member name, that denotes the expression's declaration.
    fn reference_index(&self, x: ExprId) -> Option<u32> {
        let exprs = &self.ast.exprs;
        let decl = exprs.decl(x);
        self.refs_named_at(exprs.get(x).loc())
            .find(|(_, r)| matches!(r.role, Role::Expr | Role::Member) && r.referent == decl)
            .map(|(i, _)| i)
    }

    /// The table entry of the name of `x`.
    fn reference_at(&self, x: ExprId) -> Option<&WrittenRef> {
        self.reference_index(x).map(|i| &self.refs[i as usize])
    }

    /// Whether call `e` names a function with an explicit object
    /// parameter through member syntax (`s.f(2)`): a `CallExpr` whose
    /// callee is a name in the role of a member name.
    fn explicit_object(&self, e: ExprId) -> bool {
        let exprs = &self.ast.exprs;
        if exprs.get(e).kind() != ExprKind::CallExpr {
            return false;
        }
        let c = self.strip_parens(exprs.callee(e));
        exprs.get(c).kind() == ExprKind::DeclRefExpr
            && self.reference_at(c).is_some_and(|r| r.role == Role::Member)
    }

    /// `e` without the parentheses and implicit conversions around it.
    fn strip_parens(&self, mut e: ExprId) -> ExprId {
        let exprs = &self.ast.exprs;
        loop {
            match exprs.get(e).kind() {
                ExprKind::ParenExpr | ExprKind::ImplicitCastExpr => e = exprs.operand(e),
                _ => return e,
            }
        }
    }

    /// An argument as written (rule 5): without what Kiln wraps around it.
    fn as_written_expr(&self, mut e: ExprId) -> ExprId {
        while let Some(c) = self.transparent(e) {
            e = c;
        }
        e
    }

    /// What `e` stands for when it is a node Kiln adds around a written
    /// expression, whose tokens are exactly its operand's: an implicit
    /// conversion, a temporary's materialisation, binding or cleanup, a
    /// constant's wrapper, an initializer list's backing array, a user
    /// conversion function's implicit call (its object), and an implicit
    /// converting or copying construction from one written argument.
    fn transparent(&self, e: ExprId) -> Option<ExprId> {
        let ast = self.ast;
        let exprs = &ast.exprs;
        let n = exprs.get(e);
        match n.kind() {
            ExprKind::ImplicitCastExpr
            | ExprKind::MaterializeTemporaryExpr
            | ExprKind::CXXBindTemporaryExpr
            | ExprKind::ConstantExpr
            | ExprKind::ExprWithCleanups
            | ExprKind::CXXStdInitializerListExpr => exprs.first_child(e, &ast.lists),
            ExprKind::CXXMemberCallExpr => {
                let callee = exprs.member_call_callee(e);
                let c = exprs.get(callee);
                let implicit = exprs.end_loc(e).is_none_or(|l| l == c.loc());
                (implicit
                    && c.kind() == ExprKind::MemberExpr
                    && self.kind(exprs.decl(callee)) == DeclKind::CXXConversion)
                    .then(|| exprs.member_call_object(e))
            }
            ExprKind::CXXConstructExpr => {
                if exprs.end_loc(e).is_some() {
                    return None;
                }
                let mut args = ast
                    .lists
                    .iter::<ExprId>(exprs.construct_args(e))
                    .filter(|&a| exprs.get(a).kind() != ExprKind::CXXDefaultArgExpr);
                let (Some(a), None) = (args.next(), args.next()) else { return None };
                // `h({1})` is a construction at `{` from `1`, the braces
                // being in no node: not transparent.
                (n.loc() == ast.begin_loc(a)).then_some(a)
            }
            _ => None,
        }
    }

    // ---- extents (§6.6 rules 3 and 4) ----------------------------------------

    /// The location of the first token of `e`, by the table of
    /// `spec/landed/api/WP-086.md`.
    pub(super) fn first_token(&self, mut e: ExprId) -> Result<Loc, NoExtent> {
        let ast = self.ast;
        let exprs = &ast.exprs;
        loop {
            if let Some(c) = self.transparent(e) {
                e = c;
                continue;
            }
            let n = exprs.get(e);
            let kind = n.kind();
            e = match kind {
                ExprKind::UnaryOperator if exprs.unary_op(e).is_postfix() => exprs.operand(e),
                ExprKind::CXXThisExpr => {
                    if exprs.this_is_implicit(e) {
                        return Err(NoExtent::new(kind, "implicit"));
                    }
                    return Ok(n.loc());
                }
                ExprKind::IntegerLiteral
                | ExprKind::FloatingLiteral
                | ExprKind::CharacterLiteral
                | ExprKind::StringLiteral
                | ExprKind::UnaryOperator
                | ExprKind::CXXBoolLiteralExpr
                | ExprKind::CXXNullPtrLiteralExpr
                | ExprKind::GNUNullExpr
                | ExprKind::ParenExpr
                | ExprKind::LambdaExpr
                | ExprKind::InitListExpr
                | ExprKind::CXXStaticCastExpr
                | ExprKind::CXXConstCastExpr
                | ExprKind::CXXReinterpretCastExpr
                | ExprKind::CXXDynamicCastExpr
                | ExprKind::UnaryExprOrTypeTraitExpr
                | ExprKind::CStyleCastExpr => return Ok(n.loc()),
                ExprKind::DeclRefExpr => return self.name_first(e),
                ExprKind::MemberExpr => {
                    let base = exprs.operand(e);
                    if self.is_implicit_this(base) {
                        return self.implicit_member_first(e);
                    }
                    base
                }
                ExprKind::CXXFunctionalCastExpr | ExprKind::CXXTemporaryObjectExpr => {
                    return self.type_first(e);
                }
                ExprKind::CallExpr | ExprKind::CXXMemberCallExpr => {
                    if !self.postfix(e, self.strip_parens(exprs.callee(e))) {
                        return Err(NoExtent::new(kind, "not a postfix call"));
                    }
                    if self.explicit_object(e) {
                        let first = ast.lists.at::<ExprId>(exprs.call_args(e), 0);
                        first.ok_or(NoExtent::new(kind, "no object"))?
                    } else {
                        exprs.callee(e)
                    }
                }
                ExprKind::ArraySubscriptExpr | ExprKind::BinaryOperator => exprs.operands(e)[0],
                ExprKind::CompoundAssignOperator | ExprKind::ConditionalOperator => {
                    exprs.first_child(e, &ast.lists).ok_or(NoExtent::new(kind, "no operand"))?
                }
                ExprKind::CXXOperatorCallExpr => match self.operator_form(e) {
                    Some(OpForm::Prefix) => return Ok(n.loc()),
                    Some(_) => ast
                        .lists
                        .at::<ExprId>(exprs.call_args(e), 0)
                        .ok_or(NoExtent::new(kind, "no operand"))?,
                    None => return Err(NoExtent::new(kind, "operator form")),
                },
                _ => return Err(NoExtent::new(kind, "kind outside the table")),
            };
        }
    }

    /// The location of the last token of `e`.
    pub(super) fn last_token(&self, mut e: ExprId) -> Result<Loc, NoExtent> {
        let ast = self.ast;
        let exprs = &ast.exprs;
        loop {
            if let Some(c) = self.transparent(e) {
                e = c;
                continue;
            }
            let n = exprs.get(e);
            let kind = n.kind();
            let stored = || exprs.end_loc(e).ok_or(NoExtent::new(kind, "no closing token"));
            e = match kind {
                ExprKind::IntegerLiteral
                | ExprKind::FloatingLiteral
                | ExprKind::CharacterLiteral
                | ExprKind::CXXBoolLiteralExpr
                | ExprKind::CXXNullPtrLiteralExpr
                | ExprKind::GNUNullExpr => return Ok(n.loc()),
                ExprKind::CXXThisExpr if !exprs.this_is_implicit(e) => return Ok(n.loc()),
                // A string literal of one piece has no stored end.
                ExprKind::StringLiteral => return Ok(exprs.end_loc(e).unwrap_or(n.loc())),
                ExprKind::ParenExpr
                | ExprKind::LambdaExpr
                | ExprKind::InitListExpr
                | ExprKind::CXXStaticCastExpr
                | ExprKind::CXXConstCastExpr
                | ExprKind::CXXReinterpretCastExpr
                | ExprKind::CXXDynamicCastExpr
                | ExprKind::CXXFunctionalCastExpr
                | ExprKind::CXXTemporaryObjectExpr
                | ExprKind::CallExpr
                | ExprKind::CXXMemberCallExpr
                | ExprKind::ArraySubscriptExpr => return stored(),
                ExprKind::UnaryExprOrTypeTraitExpr => match exprs.end_loc(e) {
                    Some(l) => return Ok(l),
                    None => {
                        exprs.first_child(e, &ast.lists).ok_or(NoExtent::new(kind, "no operand"))?
                    }
                },
                ExprKind::DeclRefExpr | ExprKind::MemberExpr => return self.name_last(e),
                ExprKind::UnaryOperator if exprs.unary_op(e).is_postfix() => return Ok(n.loc()),
                ExprKind::UnaryOperator | ExprKind::CStyleCastExpr => exprs.operand(e),
                ExprKind::BinaryOperator => exprs.operands(e)[1],
                ExprKind::CompoundAssignOperator => exprs.compound_assign(e).rhs,
                ExprKind::ConditionalOperator => exprs.cond_arms(e)[1],
                ExprKind::CXXOperatorCallExpr => {
                    let arg = |i: usize| {
                        ast.lists
                            .at::<ExprId>(exprs.call_args(e), i)
                            .ok_or(NoExtent::new(kind, "no operand"))
                    };
                    match self.operator_form(e) {
                        Some(OpForm::Prefix) => arg(0)?,
                        Some(OpForm::Binary) => arg(1)?,
                        Some(OpForm::Postfix) => return Ok(n.loc()),
                        Some(OpForm::Closed) => return stored(),
                        Some(OpForm::Arrow) | None => {
                            return Err(NoExtent::new(kind, "operator form"));
                        }
                    }
                }
                _ => return Err(NoExtent::new(kind, "kind outside the table")),
            };
        }
    }

    /// The form of an overloaded operator written as an operator.
    fn operator_form(&self, e: ExprId) -> Option<OpForm> {
        let exprs = &self.ast.exprs;
        let op = exprs.operator_call_op(e);
        let n = self.ast.lists.get(exprs.call_args(e)).len();
        match op {
            Op::Arrow => Some(OpForm::Arrow),
            Op::Call | Op::Subscript => Some(OpForm::Closed),
            Op::PlusPlus | Op::MinusMinus if n == 2 => Some(OpForm::Postfix),
            _ if n == 1 => Some(OpForm::Prefix),
            _ if n == 2 => Some(OpForm::Binary),
            _ => None,
        }
    }

    /// Whether `e` is the `this` Kiln makes for a member named without
    /// one.
    fn is_implicit_this(&self, mut e: ExprId) -> bool {
        let exprs = &self.ast.exprs;
        while exprs.get(e).kind() == ExprKind::ImplicitCastExpr {
            e = exprs.operand(e);
        }
        exprs.get(e).kind() == ExprKind::CXXThisExpr && exprs.this_is_implicit(e)
    }

    /// The first token of a `DeclRefExpr`: the first of its qualifier (a
    /// leading `::` included) or its name, when the written-reference
    /// table and Kiln's begin agree on it.
    fn name_first(&self, e: ExprId) -> Result<Loc, NoExtent> {
        let kind = ExprKind::DeclRefExpr;
        let r = self.reference_at(e).ok_or(NoExtent::new(kind, "no reference"))?;
        let begin = self.ast.begin_loc(e);
        let qualified = self.ast.exprs.decl_ref_extra(e).and_then(|x| x.qualifier);
        match qualified {
            None if !r.global && r.first == r.name.lo && begin == r.name.lo => Ok(begin),
            Some(q) if !r.global && r.first != r.name.lo && begin == r.first => {
                self.qualifier_linked(q).then_some(begin).ok_or(NoExtent::new(kind, "qualifier"))
            }
            // `::f`: Kiln's begin is the `::` (the parser's qualifier
            // begin); the table's first is the component after it.
            Some(q) if r.global && self.order(begin, r.first).is_lt() => {
                self.qualifier_linked(q).then_some(begin).ok_or(NoExtent::new(kind, "qualifier"))
            }
            _ => Err(NoExtent::new(kind, "qualifier")),
        }
    }

    /// The first token of a member named without `this`: the first of its
    /// qualifier, or its name. Kiln's begin of such a node is the name.
    fn implicit_member_first(&self, e: ExprId) -> Result<Loc, NoExtent> {
        let kind = ExprKind::MemberExpr;
        let r = self.reference_at(e).ok_or(NoExtent::new(kind, "no reference"))?;
        let qualified = self.ast.exprs.decl_ref_extra(e).and_then(|x| x.qualifier);
        match qualified {
            None if !r.global && r.first == r.name.lo => Ok(r.name.lo),
            Some(q) if !r.global && r.first != r.name.lo && self.qualifier_linked(q) => Ok(r.first),
            _ => Err(NoExtent::new(kind, "qualifier")),
        }
    }

    /// Whether every component of qualifier `q` is one the table links
    /// (a namespace, or a type named by a name): a `decltype` or a splice
    /// ends the table's chain, so its first would be too late.
    fn qualifier_linked(&self, q: slip_kiln::kiln::ast::NnsId) -> bool {
        let types = &self.ast.types;
        let mut at = Some(q);
        for _ in 0..self.chain_bound() {
            let Some(c) = at else { return true };
            at = match types.nns_view(c) {
                Nns::Global => None,
                Nns::Namespace { prefix, .. } | Nns::NamespaceAlias { prefix, .. } => prefix,
                Nns::Type { prefix, ty, .. } => {
                    if matches!(
                        types.kind(ty.ty()),
                        TypeKind::Decltype
                            | TypeKind::DependentDecltype
                            | TypeKind::TypeOf
                            | TypeKind::PackIndexing
                            | TypeKind::UnaryTransform
                            | TypeKind::Splice
                    ) {
                        return false;
                    }
                    prefix
                }
                _ => return false,
            };
        }
        false
    }

    /// The last token of a name (`DeclRefExpr`, `MemberExpr`): its name,
    /// when that is one token and no template argument list follows.
    fn name_last(&self, e: ExprId) -> Result<Loc, NoExtent> {
        let kind = self.ast.exprs.get(e).kind();
        if self.ast.exprs.decl_ref_extra(e).is_some_and(|x| x.template_args.is_some()) {
            return Err(NoExtent::new(kind, "template arguments"));
        }
        let r = self.reference_at(e).ok_or(NoExtent::new(kind, "no reference"))?;
        if self.tokens.token_end(r.name.lo) != Some(r.name.hi) {
            return Err(NoExtent::new(kind, "name of several tokens"));
        }
        Ok(r.name.lo)
    }

    /// The first token of `T(x)` or `T{x}`: Kiln's location, the type's
    /// first token, when the table has the type's name there unqualified
    /// by `::` (or the type is a keyword).
    fn type_first(&self, e: ExprId) -> Result<Loc, NoExtent> {
        let ast = self.ast;
        let n = ast.exprs.get(e);
        let at = n.loc();
        let named = self
            .refs
            .iter()
            .any(|r| r.first == at && !r.global && matches!(r.role, Role::Type | Role::Template));
        // A type written with keywords (`int(2)`, `unsigned(3)`): the
        // node's type is the builtin type itself, no sugar (a typedef, a
        // `decltype` keeps its sugar on the node).
        let keyword = n.kind() == ExprKind::CXXFunctionalCastExpr
            && ast.types.kind(n.ty().ty()) == TypeKind::Builtin;
        if named || keyword { Ok(at) } else { Err(NoExtent::new(n.kind(), "type not named")) }
    }

    /// The range of `e`'s tokens, or why there is none.
    pub(super) fn expr_range(&self, e: ExprId) -> Result<Range, NoExtent> {
        let kind = self.ast.exprs.get(e).kind();
        let first = self.first_token(e)?;
        let last = self.last_token(e)?;
        let r = ranges::file_range_of_tokens(&self.tokens, first, last)
            .ok_or(NoExtent::new(kind, "no range of tier 0 or 1"))?;
        self.subject_range(r.file, r.lo, r.hi)
            .ok_or(NoExtent::new(kind, "not a file of the subject"))
    }

    /// The range of a file range, when its file is a file of the subject
    /// whose bytes are Kiln's text (WP-084 point 6).
    fn subject_range(
        &self,
        file: slip_kiln::kiln::source::FileId,
        lo: u32,
        hi: u32,
    ) -> Option<Range> {
        let f = File(file.index() as u32 + 1);
        let (_, bytes) = self.file(f)?;
        matches!(bytes, std::borrow::Cow::Borrowed(_)).then_some(Range { file: f, lo, hi })
    }

    /// The tier of the range of `e`, with the range.
    fn expr_tier(&self, e: ExprId) -> Option<(Range, Tier)> {
        let first = self.first_token(e).ok()?;
        let last = self.last_token(e).ok()?;
        let r = ranges::file_range_of_tokens(&self.tokens, first, last)?;
        Some((self.subject_range(r.file, r.lo, r.hi)?, r.tier))
    }

    // ---- what the subject answers --------------------------------------------

    /// Whether expression `n` is live: a call or an argument of one.
    pub(super) fn expression_live(&self, n: u32) -> bool {
        self.call_index().holder.contains_key(&n)
    }

    /// Whether expression `n` is a call.
    pub(super) fn is_call(&self, n: u32) -> bool {
        self.call_index().by_expr.contains_key(&n)
    }

    fn call(&self, x: Expression) -> Option<&Call> {
        let index = self.call_index();
        index.by_expr.get(&x.0).map(|&i| &index.calls[i as usize])
    }

    /// §6.7 rule 3: the calls whose callee's origin is `origin`.
    pub(super) fn calls_of(&self, origin: DeclId) -> Vec<Expression> {
        let index = self.call_index();
        index.by_origin.get(&origin.index()).map_or_else(Vec::new, |v| {
            v.iter().map(|&i| Expression(index.calls[i as usize].expr.index())).collect()
        })
    }

    /// §6.7 rule 4: the function called.
    pub(super) fn callee_of(&self, x: Expression) -> Option<DeclId> {
        self.call(x).map(|c| self.entity(c.callee))
    }

    /// §6.7 rule 4: the callee's reference.
    pub(super) fn callee_reference_of(&self, x: Expression) -> Option<Reference> {
        self.call(x).map(|c| Reference(c.reference + 1))
    }

    /// §6.7 rule 5.
    pub(super) fn arguments_of(&self, x: Expression) -> Option<Vec<Expression>> {
        self.call(x).map(|c| c.args.iter().map(|a| Expression(a.index())).collect())
    }

    /// §6.6 rule 3: the extent of a live expression.
    pub(super) fn expression_extent(&self, n: u32) -> Option<Range> {
        let e = self.expr(n)?;
        self.expr_range(e).ok()
    }

    /// The tiers of `r` when it is the extent of a live expression.
    pub(super) fn expression_tiers(&self, r: Range) -> Option<[bool; 2]> {
        self.call_tiers
            .get_or_init(|| {
                let mut map: HashMap<Range, [bool; 2]> = HashMap::new();
                for &n in self.call_index().holder.keys() {
                    if let Some((r, t)) = self.expr(n).and_then(|e| self.expr_tier(e)) {
                        map.entry(r).or_default()[t as usize] = true;
                    }
                }
                map
            })
            .get(&r)
            .copied()
    }

    /// §7.4 rule 3 for an expression: its extent; else the outermost macro
    /// invocation its first token (or Kiln's location for it) came from;
    /// else unnamed.
    pub(super) fn expression_place(&self, n: u32) -> PlaceOf {
        let Some(e) = self.expr(n) else { return PlaceOf::Unnamed };
        if let Ok(r) = self.expr_range(e) {
            return PlaceOf::Own(r);
        }
        let at = self.first_token(e).unwrap_or_else(|_| self.ast.exprs.get(e).loc());
        if !at.is_macro() {
            return PlaceOf::Unnamed;
        }
        let span: Span = self.sm.expansion_span(at);
        match ranges::file_range_of_span(self.sm, span)
            .and_then(|r| self.subject_range(r.file, r.lo, r.hi))
        {
            Some(r) => PlaceOf::Nearest(r),
            None => PlaceOf::Unnamed,
        }
    }

    /// A message's name for expression `n`: what it is and where. Never
    /// the text of the code.
    pub(super) fn expression_display(&self, n: u32) -> String {
        let Some(e) = self.expr(n) else { return format!("expression {n}") };
        let what = if self.is_call(n) { "a call" } else { "an expression" };
        let at = self.first_token(e).unwrap_or_else(|_| self.ast.begin_loc(e));
        format!("{what} at {}", self.describe_loc(self.sm.expansion_loc(at)))
    }

    /// The `ExprId` numbered `n`.
    fn expr(&self, n: u32) -> Option<ExprId> {
        (n >= 1 && n as usize <= self.ast.exprs.len()).then(|| ExprId::new(n))
    }

    /// Whether reference `n` is live: an entry of the table in a role of
    /// §6.5 rule 3 whose referent is a live entity. INTERIM: WP-085 owns
    /// references and replaces this with its own rule; the numbering
    /// (index in `refs`, plus one) is the one this package gives out.
    pub(super) fn reference_live(&self, n: u32) -> bool {
        let Some(r) = (n as usize).checked_sub(1).and_then(|i| self.refs.get(i)) else {
            return false;
        };
        matches!(
            r.role,
            Role::Expr
                | Role::Member
                | Role::Type
                | Role::Template
                | Role::Nns
                | Role::Base
                | Role::MemInit
        ) && self.is_entity(self.entity(r.referent))
    }

    /// The referent of live reference `n` (§6.5 rule 2).
    pub(super) fn reference_referent(&self, n: u32) -> Option<DeclId> {
        let r = (n as usize).checked_sub(1).and_then(|i| self.refs.get(i))?;
        Some(self.entity(r.referent))
    }
}

/// The forms of an overloaded operator written as an operator.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OpForm {
    /// `-a`, `++a`: the operator, then the operand.
    Prefix,
    /// `a++`: the operand, then the operator.
    Postfix,
    /// `a + b`.
    Binary,
    /// `a(x)`, `a[i]`: the operand, then a closing token Kiln stores.
    Closed,
    /// `a->m`: the operand, then `->`; the member's name, the last token
    /// of the member access, is not the operator call's.
    Arrow,
}

impl KilnSubject<'_> {
    /// The written declaration whose body or initialiser holds `x` (a live
    /// expression): the declaration at namespace or class scope, as
    /// written (a template for its pattern), that the walk entered to
    /// reach it. WP-095 turns it into a unit key.
    #[allow(dead_code, reason = "WP-095 is its caller")]
    pub(crate) fn holder(&self, x: Expression) -> Option<DeclId> {
        self.call_index().holder.get(&x.0).copied()
    }

    /// How many expressions of §6.7 rule 1's shape were not made calls
    /// because the callee's name has no entry in the written-reference
    /// table (`api.calls_without_reference`; the `Subject` trait has no
    /// counter yet: proposed in the hand-back of WP-086).
    pub fn calls_without_reference(&self) -> u64 {
        self.call_index().without_reference
    }

    /// Why each call and each argument of a call has no extent.
    pub fn extent_census(&self) -> ExtentCensus {
        let index = self.call_index();
        let mut c =
            ExtentCensus { without_reference: index.without_reference, ..Default::default() };
        for call in &index.calls {
            c.calls += 1;
            if let Err(w) = self.expr_range(call.expr) {
                *c.calls_null.entry(w).or_default() += 1;
            }
            for &a in &call.args {
                c.arguments += 1;
                if let Err(w) = self.expr_range(a) {
                    *c.arguments_null.entry(w).or_default() += 1;
                }
            }
        }
        c
    }

    /// Every call in unit order, with the subject path and byte offset of
    /// the `)` Kiln stores for it when that token was spelled in a file
    /// rather than produced by a macro. For tests: the differential
    /// against another front end keys calls by their `)`.
    pub fn call_sites(&self) -> Vec<CallSite<'_>> {
        self.call_index()
            .calls
            .iter()
            .map(|c| {
                let close = self.ast.exprs.end_loc(c.expr).filter(|l| !l.is_macro());
                let at = close
                    .and_then(|l| self.sm.offset_of(l))
                    .and_then(|(f, o)| Some((self.path(f)?, o)));
                (Expression(c.expr.index()), at)
            })
            .collect()
    }

    /// The subject path of file `f`. For tests.
    pub fn file_path(&self, f: File) -> Option<&[u8]> {
        self.file(f).map(|(p, _)| &p[..])
    }

    /// Why expression `x` has no extent (`None` when it has one, or is
    /// not live). For tests.
    pub fn no_extent(&self, x: Expression) -> Option<NoExtent> {
        if !self.expression_live(x.0) {
            return None;
        }
        self.expr_range(self.expr(x.0)?).err()
    }
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::DeclId;
    use slip_kiln::snippet::{Extra, Snippet, parse_with_tables};

    use super::super::KilnSubject;
    use crate::subject::Expression;

    /// `holder`: the namespace- or class-scope declaration as written that
    /// holds each call; a local class's, a lambda's, a local variable's
    /// and a condition variable's calls are their enclosing function's.
    #[test]
    fn holder_of_each_call() {
        let src = "\
int f(int);
int a = f(1);
struct S { int m() { struct L { int n() { return f(2); } }; int v = f(3); if (int c = f(7)) {} return [] { return f(4); }(); } int k = f(5); };
namespace N { template <class T> int t() { return f(6); } }
";
        let files = [("main.cpp", src)];
        let snippet = Snippet { files: &files, main: "main.cpp", include: &[], std: "c++20" };
        parse_with_tables(&snippet, &Extra::default(), |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let owned = |_: &[u8]| true;
            let s = KilnSubject::from_snippet(p, &owned);
            let index = s.call_index();
            let name = |d: DeclId| String::from_utf8(s.identifier_of(d)).unwrap();
            let holders: Vec<(String, String)> = index
                .calls
                .iter()
                .map(|c| {
                    let arg = c.args[0];
                    let h = s.holder(Expression(c.expr.index())).unwrap();
                    assert_eq!(s.holder(Expression(arg.index())), Some(h));
                    (format!("{:?}", s.kind(h)), name(h))
                })
                .collect();
            let want = [
                ("Var", "a"),
                ("CXXMethod", "m"),
                ("CXXMethod", "m"),
                ("CXXMethod", "m"),
                ("CXXMethod", "m"),
                ("Field", "k"),
                ("FunctionTemplate", "t"),
            ];
            let want: Vec<(String, String)> =
                want.iter().map(|(k, n)| ((*k).to_owned(), (*n).to_owned())).collect();
            assert_eq!(holders, want);
        })
        .unwrap();
    }
}
