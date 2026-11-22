//! The payload records of scopes and of the declarations that declare no
//! name, and the cold maps of declarations (`spec/digest-kinds.md` §5.6
//! to §5.8).
//!
//! Each function writes one `record T` of §5.6 for the declaration `d`,
//! read as §2.3 says; `cold` writes the rows of `ColdDeclData` (§5.7).
//! §5.8 (the redeclaration and lookup tables) writes nothing: every row
//! is `out`. `ScopeData` `aux-lang` and `aux-braceless` are written in the
//! stream of each function and variable (`decl_func::linkage`).
//!
//! Which declaration each cold map is read from is that of Kiln's own
//! readers and writers (`spec/landed/digest-kinds/WP-033.md`): the
//! declaration the walk reads, the first declaration, or the first
//! declaration of the chain that has an entry.

use slip_kiln::kiln::ast::decl::{
    ColdDeclData, DeclKind, FriendTarget, NamespaceData, StaticAssertData,
};
use slip_kiln::kiln::ast::fp::FpOverride;
use slip_kiln::kiln::ast::slot::ColdMap;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{DeclId, StmtId};

use crate::keys::{Role, info};
use crate::refs::REF_NONE;
use crate::stream::{FlagWord, Stream};
use crate::tables::friend_target;
use crate::walk::Walk;
use crate::{Area, Cause};

/// `NamespaceData` `flags` holds a bit no row names.
const NAMESPACE_FLAGS: Cause = Cause::Unaccounted("NamespaceData/flags");
/// A `Namespace` with no `NamespaceData`.
const NAMESPACE_MISSING: Cause = Cause::Unaccounted("NamespaceData/missing");
/// A `StaticAssert` or `ConstevalBlock` with no `StaticAssertData`.
const STATIC_ASSERT_MISSING: Cause = Cause::Unaccounted("StaticAssertData/missing");
/// A `FileScopeAsm` with no string.
const ASM_MISSING: Cause = Cause::Unaccounted("DeclNode/FileScopeAsm.data");
/// An entry of the map no site in Kiln fills (§5.7).
const INSTANTIATED_FROM: Cause = Cause::Unaccounted("ColdDeclData/instantiated_from");
/// The implicit variable of a file-scope compound literal (§5.7).
const COMPOUND_LITERAL: Cause = Cause::Unsupported("ColdDeclData/compound_literal");
/// How far a chain of contexts is followed (an AST is far shallower).
const CTX_LIMIT: u32 = 4096;

/// enum `FriendTarget` of the `Friend` `d`: the befriended declaration as
/// a `ref`, or the befriended type. `Type.owned_tag` is `out`.
pub(crate) fn friend_target(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    if ast.decls.get(d).kind() != DeclKind::Friend {
        // `DeclArena::friend_target` panics on another kind.
        s.uv(0).indeterminate(Cause::Unaccounted("FriendTarget/kind"));
        return;
    }
    let target = ast.decls.friend_target(d);
    s.tag(friend_target::row(&target));
    match target {
        FriendTarget::Decl(x) => w.write_ref(s, x),
        FriendTarget::Type { ty, owned_tag: _ } => w.write_type(s, ty),
    }
}

/// `NamespaceData` of the namespace `d`: `flags-inline` as one bit, set
/// if any declaration of the namespace says `inline` (a reopening need
/// not repeat it, [namespace.def]/5). `redecl` and `members` are `out`,
/// `flags-anonymous` is `keyed`.
pub(crate) fn namespace_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let mut inline = false;
    let mut found = false;
    for x in w.redecls(ast.first(d)) {
        let Some(&NamespaceData { redecl: _, members: _, flags }) = ast.decls.namespace(x) else {
            continue;
        };
        found = true;
        if flags & !(NamespaceData::INLINE | NamespaceData::ANONYMOUS) != 0 {
            s.indeterminate(NAMESPACE_FLAGS);
        }
        inline |= flags & NamespaceData::INLINE != 0;
    }
    if !found {
        s.indeterminate(NAMESPACE_MISSING);
    }
    s.flags(FlagWord::new().bit(inline));
}

/// `StaticAssertData` of `d`: `cond` and `message` as children.
pub(crate) fn static_assert_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let Some(&StaticAssertData { cond, message }) = w.ast.decls.static_assert(d) else {
        s.uv(0).indeterminate(STATIC_ASSERT_MISSING);
        return;
    };
    crate::expr::child(w, s, cond);
    crate::expr::child_opt(w, s, message);
}

/// `FileScopeAsm.data`: the string literal, a `child`.
pub(crate) fn file_scope_asm(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    match w.ast.file_scope_asm_string(d) {
        Some(e) => crate::expr::child(w, s, e),
        None => {
            s.uv(0).indeterminate(ASM_MISSING);
        }
    }
}

/// The rows of `ColdDeclData` (§5.7) for the declaration `d` the walk
/// reads, in table order. An absent entry writes the empty form of the
/// row's encoding; `out`, `demand` and `keyed` rows write nothing.
#[allow(clippy::too_many_lines, reason = "one block per row of §5.7 ColdDeclData")]
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n node, q type"
)]
pub(crate) fn cold(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let ColdDeclData {
        lexical_ctx: _,
        begin_loc: _,
        delete_message,
        explicit_expr,
        ctor_inits,
        overrides,
        friends,
        key_function: _,
        dtor_delete: _,
        dtor_delete_this: _,
        record_pack,
        contracts: _,
        asm_label,
        alignment,
        deprecation,
        instantiated_from,
        injection_point: _,
        declarator_end: _,
        builtin_id: _,
        gnu_lib_twin,
        lib_builtin: _,
        typedef_for_anon: _,
        decomp_bindings,
        binding_decomp: _,
        binding_expr,
        binding_var,
        ctor_shadow_nominated,
        inheriting_ctor: _,
        inherited_ctor: _,
        hidden_shadow,
        conversion_spec_target: _,
        compound_literal,
        init_fp: _,
    } = &ast.decls.cold;
    let e = w.entity(d);
    // The chain of `d`'s declarations, the first first (§2.3).
    let mut chain = w.redecls(ast.first(d));
    chain.reverse();
    // lexical_ctx: ref? (rule under the table).
    lexical(w, s, d);
    // begin_loc: out.
    // delete_message, explicit_expr: child?, from the first declaration
    // of the chain that has one (Kiln puts the first on `f` and its first
    // declaration, the second on the in-class declaration).
    let x = from_chain(delete_message, &chain);
    crate::expr::child_opt(w, s, x);
    let x = from_chain(explicit_expr, &chain);
    crate::expr::child_opt(w, s, x);
    // ctor_inits: record CtorInit*, on the definition; in `definition`
    // for the definition class (§11.9).
    // The empty form carries `Pending(Demand)` for every constructor and
    // class of the class, whether Kiln made the definition or not: what
    // the unit leaves out is not yet anywhere.
    let definition_class = crate::decl_func::definition_class(w, e);
    let ctor = ast.decls.get(d).kind() == DeclKind::CXXConstructor;
    let class = ast.decls.record(d).is_some();
    if definition_class {
        s.count(0);
        if ctor || ctor_inits.contains(d) {
            s.pending(Area::Demand);
        }
    } else {
        let inits: Vec<_> = ast.ctor_inits(d).map(|i| i.init).collect();
        s.count(inits.len());
        for init in inits {
            // The `CtorInit` record is the class area's
            // (`decl_class::ctor_init`); its `init` is reached here.
            s.pending(Area::DeclData);
            crate::expr::child(w, s, init);
        }
    }
    // overrides: ref*sorted, from the first declaration.
    let list = overrides.get(ast.first(d)).copied();
    let ids: Vec<DeclId> = list.map(|l| ast.lists.iter::<DeclId>(l).collect()).unwrap_or_default();
    w.write_refs_sorted(s, &ids);
    // friends: key*sorted, of the class definition; in `definition` for
    // the definition class (§11.9).
    if definition_class {
        s.count(0);
        if class || friends.contains(d) {
            s.pending(Area::Demand);
        }
    } else {
        let list = friends.get(d).copied();
        let ids: Vec<DeclId> =
            list.map(|l| ast.lists.iter::<DeclId>(l).collect()).unwrap_or_default();
        write_keys_sorted(w, s, &ids);
    }
    // key_function, dtor_delete, dtor_delete_this: demand.
    // record_pack: uv?, of the class definition; in `definition` for the
    // definition class (§11.9).
    if definition_class {
        s.opt(false);
        if class || record_pack.contains(d) {
            s.pending(Area::Demand);
        }
    } else {
        match record_pack.get(d) {
            Some(&n) => {
                s.opt(true).uv(u64::from(n));
            }
            None => {
                s.opt(false);
            }
        }
    }
    // contracts: child*, in order (Kiln's `Ast::function_contracts`).
    let stmts: Vec<StmtId> = ast
        .function_contracts(d)
        .map(|l| ast.lists.iter::<StmtId>(l).collect())
        .unwrap_or_default();
    s.count(stmts.len());
    for st in stmts {
        crate::stmt::child(w, s, st);
    }
    // asm_label: string?, on the first declaration.
    match from_chain(asm_label, &chain) {
        Some(l) => {
            s.opt(true);
            crate::value::string(w, s, l);
        }
        None => {
            s.opt(false);
        }
    }
    // alignment: child?.
    let x = from_chain(alignment, &chain);
    crate::expr::child_opt(w, s, x);
    // deprecation: string? (no site in Kiln fills it).
    match from_chain(deprecation, &chain) {
        Some(l) => {
            s.opt(true);
            crate::value::string(w, s, l);
        }
        None => {
            s.opt(false);
        }
    }
    // instantiated_from: ref. No site in Kiln fills it; an entry is a
    // shape no rule has met.
    match instantiated_from.get(d) {
        Some(&x) => {
            w.write_ref(s, x);
            s.indeterminate(INSTANTIATED_FROM);
        }
        None => {
            s.uv(REF_NONE);
        }
    }
    // injection_point, declarator_end: out. builtin_id: demand.
    // gnu_lib_twin: bool, on the first declaration.
    s.bool(from_chain(gnu_lib_twin, &chain).is_some());
    // lib_builtin: demand. typedef_for_anon: keyed.
    // decomp_bindings: key* at namespace scope, decl* in a block.
    let bindings: Vec<DeclId> =
        decomp_bindings.get(d).map(|&l| ast.lists.iter::<DeclId>(l).collect()).unwrap_or_default();
    s.count(bindings.len());
    let local = w.is_local(d);
    for b in bindings {
        if local {
            crate::local::write_decl(w, s, b);
        } else {
            w.write_key(s, b);
        }
    }
    // binding_decomp: out.
    // binding_expr: child?.
    let x = binding_expr.get(d).copied();
    crate::expr::child_opt(w, s, x);
    // binding_var: decl?.
    match binding_var.get(d) {
        Some(&v) => {
            s.opt(true);
            crate::local::write_decl(w, s, v);
        }
        None => {
            s.opt(false);
        }
    }
    // ctor_shadow_nominated: ref.
    let x = ctor_shadow_nominated.get(d).copied();
    w.write_ref_opt(s, x);
    // inheriting_ctor, inherited_ctor: demand.
    // hidden_shadow: ref.
    let x = hidden_shadow.get(d).copied();
    w.write_ref_opt(s, x);
    // conversion_spec_target: out.
    // compound_literal: unsupported.
    if compound_literal.contains(d) {
        s.indeterminate(COMPOUND_LITERAL);
    }
    // init_fp: record FpOverride?.
    match init_fp(w, d) {
        Some(o) => {
            s.opt(true);
            crate::fp::fp_override(s, o);
        }
        None => {
            s.opt(false);
        }
    }
}

/// The value of the first declaration of `chain` that has an entry in
/// `map`.
fn from_chain<V: Copy>(map: &ColdMap<DeclId, V>, chain: &[DeclId]) -> Option<V> {
    chain.iter().find_map(|&x| map.get(x).copied())
}

/// `key*sorted`: each key encoded alone, sorted by its bytes, duplicates
/// removed.
fn write_keys_sorted(w: &mut Walk<'_>, s: &mut Stream, ds: &[DeclId]) {
    let mut encoded: Vec<Vec<u8>> = Vec::with_capacity(ds.len());
    for &d in ds {
        let mut one = Stream::new();
        w.write_key(&mut one, d);
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

/// `lexical_ctx` (rule under §5.7's table): present when the nearest
/// non-transparent context around the place `d` was written is not the
/// entity's semantic context, as a `Ref` to it; `Ref::none` when that
/// context is the global namespace, which has no key.
///
/// When the entity has no definition the walk reads its first
/// declaration, and which declaration came first is not a fact (digest
/// §2.1): the row is then present only if every declaration of the
/// entity is written outside its semantic context, so that `void f();
/// struct C { friend void f(); };` and its reverse are equal.
fn lexical(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let written = written_scope(w, d);
    let semantic = semantic_scope(w, d);
    let outside = |x: DeclId| written_scope(w, x) != semantic_scope(w, x);
    let present = written != semantic
        && (crate::units::defining_decl(w, ast.first(d)).is_some()
            || w.redecls(ast.first(d)).into_iter().all(outside));
    if !present {
        s.opt(false);
        return;
    }
    s.opt(true);
    w.write_ref_opt(s, written);
}

/// The entity of the nearest context that lexically encloses `d` and is
/// not transparent (§5.2), `None` for the translation unit.
fn written_scope(w: &Walk<'_>, d: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let mut c = ast.decls.lexical_ctx(d);
    for _ in 0..CTX_LIMIT {
        let x = c?;
        match info(ast.decls.get(x).kind()).role {
            Role::Entity(_) | Role::Content(_) => return Some(w.entity(x)),
            Role::Root => return None,
            Role::Transparent
            | Role::Nothing
            | Role::Local
            | Role::InParent
            | Role::Unsupported(_) => c = ast.decls.lexical_ctx(x),
        }
    }
    None
}

/// The entity of `d`'s semantic context, through transparent scopes:
/// for a block-scope function or `extern` variable the innermost
/// enclosing namespace (§2.4, [basic.link]/7), `None` for the global
/// namespace.
fn semantic_scope(w: &Walk<'_>, d: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let local_extern = w.is_local_extern(d);
    let mut c = ast.decls.get(d).ctx();
    for _ in 0..CTX_LIMIT {
        let x = c?;
        let role = info(ast.decls.get(x).kind()).role;
        let stop = match role {
            Role::Root => return None,
            Role::Entity(_) | Role::Content(_) => {
                !local_extern || ast.decls.get(x).kind() == DeclKind::Namespace
            }
            Role::Transparent
            | Role::Nothing
            | Role::Local
            | Role::InParent
            | Role::Unsupported(_) => false,
        };
        if stop {
            return Some(w.entity(x));
        }
        c = ast.decls.get(x).ctx();
    }
    None
}

/// `init_fp` of `d`, absent when it is the command line's: for a
/// parameter, that of the declaration whose default argument the
/// parameter's stream writes (Kiln's `Ast::default_arg_fp`), none for a
/// parameter of an instantiated function, whose default argument is a
/// derived unit's (§11.4); else Kiln's `Ast::init_fp`, which reads the
/// chain and, for an instantiation, its pattern.
fn init_fp(w: &Walk<'_>, d: DeclId) -> Option<FpOverride> {
    let ast = w.ast;
    let o = if ast.decls.param(d).is_some() {
        let function = ast.decls.get(d).ctx().filter(|&f| ast.decls.function(f).is_some());
        if function.is_some_and(|f| instantiated(w, f)) {
            return None;
        }
        ast.default_arg_fp(d)
    } else {
        ast.init_fp(d)
    };
    (o != FpOverride::NONE).then_some(o)
}

/// Whether the function declaration `f` was made by instantiation: its
/// own `SpecInfo` kind is an instantiation of any form, or that of the
/// nearest lexically enclosing context with a kind is (the reading of
/// `ParamData` `default_arg`, `decl_func.rs`).
fn instantiated(w: &Walk<'_>, f: DeclId) -> bool {
    let ast = w.ast;
    let instantiation = |k: SpecKind| match k {
        SpecKind::ImplicitInstantiation
        | SpecKind::ExplicitInstantiationDeclaration
        | SpecKind::ExplicitInstantiationDefinition => true,
        SpecKind::Undeclared | SpecKind::ExplicitSpecialization => false,
    };
    if let Some((_, i)) = ast.spec_info(f) {
        return instantiation(i.kind);
    }
    let mut c = ast.decls.lexical_ctx(f);
    for _ in 0..64 {
        let Some(y) = c else { return false };
        let kind = ast
            .spec_info(y)
            .map(|(_, i)| i.kind)
            .or_else(|| ast.member_spec_info(y).map(|(_, i)| i.kind));
        if let Some(k) = kind {
            return instantiation(k);
        }
        c = ast.decls.lexical_ctx(y);
    }
    false
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::DeclId;
    use slip_kiln::kiln::ast::decl::DeclKind;

    use crate::decl_func::tests::{named, of_kind, with_walk};
    use crate::stream::Stream;
    use crate::walk::Walk;
    use crate::{Area, Cause};

    fn written(
        w: &mut Walk<'_>,
        f: impl FnOnce(&mut Walk<'_>, &mut Stream),
    ) -> (Vec<u8>, Option<Cause>) {
        let mut s = Stream::new();
        f(w, &mut s);
        (s.as_bytes().to_vec(), s.cause())
    }

    /// The bytes `lexical` writes for each declaration of `kind` named
    /// `name`, in arena order.
    fn lexical_of(src: &str, name: &str, kind: DeclKind) -> Vec<Vec<u8>> {
        with_walk(src, |w| {
            named(w, name, kind)
                .into_iter()
                .map(|d| written(w, |w, s| super::lexical(w, s, d)).0)
                .collect()
        })
    }

    const ABSENT: &[u8] = &[0];

    // §5.7 `lexical_ctx`, rule under the table: the four present cases.
    #[test]
    fn lexical_ctx_is_present_where_the_rule_says() {
        // A friend defined in the class.
        let v = lexical_of("struct X { friend void f() {} };", "f", DeclKind::Function);
        assert_ne!(v[0], ABSENT);
        // A friend only declared in the class, with no other declaration.
        let v = lexical_of("struct X { friend void f(); };", "f", DeclKind::Function);
        assert_ne!(v[0], ABSENT);
        // A function and an `extern` variable declared in a block.
        let v = lexical_of("void h() { void g(); }", "g", DeclKind::Function);
        assert_ne!(v[0], ABSENT);
        let v = lexical_of("void h() { extern int v; }", "v", DeclKind::Var);
        assert_ne!(v[0], ABSENT);
        // A member defined outside its class: the global namespace has no
        // key, `Ref::none`.
        let v = lexical_of("struct X { void m(); }; void X::m() {}", "m", DeclKind::CXXMethod);
        assert_eq!(v[0], ABSENT, "the in-class declaration");
        assert_eq!(v[1], [1, crate::refs::REF_NONE as u8], "the definition");
    }

    // §5.7 `lexical_ctx`: the equal cases.
    #[test]
    fn lexical_ctx_is_absent_through_transparent_scopes() {
        let v = lexical_of("struct X { void m() {} };", "m", DeclKind::CXXMethod);
        assert_eq!(v[0], ABSENT);
        let v = lexical_of("extern \"C\" { int c; }", "c", DeclKind::Var);
        assert_eq!(v[0], ABSENT);
        let v =
            lexical_of("namespace N { extern \"C++\" { struct S {}; } }", "S", DeclKind::CXXRecord);
        assert_eq!(v[0], ABSENT);
        let v = lexical_of("struct X { friend void f(); }; void f() {}", "f", DeclKind::Function);
        assert_eq!(v[1], ABSENT, "the definition, at namespace scope");
    }

    // §5.7 `lexical_ctx` for an entity with no definition: independent of
    // the order of its declarations (digest §2.1).
    #[test]
    fn lexical_ctx_of_an_undefined_entity_ignores_declaration_order() {
        let a = lexical_of("void f(); struct C { friend void f(); };", "f", DeclKind::Function);
        let b = lexical_of("struct C { friend void f(); }; void f();", "f", DeclKind::Function);
        assert_eq!(a[0], ABSENT);
        assert_eq!(b[0], ABSENT, "the first declaration is the friend");
    }

    // §5.7 `instantiated_from`: no site in Kiln fills the map; an entry
    // would be a shape the walk has not met.
    #[test]
    fn instantiated_from_is_never_filled() {
        let src = "template <class T> struct A { T t; void f() {} static int s; struct N {}; };\n\
                   template <class T> int A<T>::s = 0;\n\
                   template <class T> T v = T();\n\
                   template <class T> void g(T) {}\n\
                   template struct A<long>;\n\
                   int use() { A<int> a; a.f(); g(1); A<char>::N n; return v<int> + A<int>::s; }\n";
        with_walk(src, |w| {
            assert!(w.ast.decls.cold.instantiated_from.is_empty());
        });
    }

    // §5.6 `NamespaceData` `flags-inline`: set if any declaration says
    // `inline`.
    #[test]
    fn namespace_inline_is_merged_over_reopenings() {
        let src = "inline namespace N { int a; }\nnamespace N { int b; }\nnamespace M { int c; }\n";
        with_walk(src, |w| {
            let ns = named(w, "N", DeclKind::Namespace);
            assert_eq!(ns.len(), 2);
            let first = written(w, |w, s| super::namespace_data(w, s, ns[0]));
            let second = written(w, |w, s| super::namespace_data(w, s, ns[1]));
            assert_eq!(first, second);
            assert_eq!(first, (vec![1], None));
            let m = named(w, "M", DeclKind::Namespace)[0];
            assert_eq!(written(w, |w, s| super::namespace_data(w, s, m)), (vec![0], None));
        });
    }

    // `from_chain`: the first declaration of the chain with an entry.
    #[test]
    fn asm_label_is_read_from_the_first_declaration() {
        let src = "extern int x asm(\"y\");\nextern int x;\nint x = 1;\n";
        with_walk(src, |w| {
            let xs = named(w, "x", DeclKind::Var);
            let mut chain = w.redecls(w.ast.first(xs[2]));
            chain.reverse();
            assert_eq!(chain, xs);
            assert!(super::from_chain(&w.ast.decls.cold.asm_label, &chain).is_some());
            assert!(super::from_chain(&w.ast.decls.cold.asm_label, &chain[1..]).is_none());
        });
    }

    // `key*sorted`: the order of the friends is not written.
    #[test]
    fn friends_are_sorted_and_deduplicated() {
        with_walk("void f(); void g();", |w| {
            let f = named(w, "f", DeclKind::Function)[0];
            let g = named(w, "g", DeclKind::Function)[0];
            let one = written(w, |w, s| super::write_keys_sorted(w, s, &[f, g]));
            let two = written(w, |w, s| super::write_keys_sorted(w, s, &[g, f, g]));
            assert_eq!(one, two);
            assert_eq!(one.0[0], 2, "the count");
        });
    }

    // §11.9: `ctor_inits` of a constructor of the definition class is in
    // `definition`, whether or not Kiln made it.
    #[test]
    fn ctor_inits_of_an_instantiation_is_pending_demand() {
        let src = "template <class T> struct A { int a; A() : a(1) {} };\nA<int> x;\n";
        with_walk(src, |w| {
            let inst: Vec<DeclId> = of_kind(w, DeclKind::CXXConstructor)
                .into_iter()
                .filter(|&c| crate::decl_func::implicit_instantiation(w, w.entity(c)))
                .collect();
            assert!(!inst.is_empty());
            for c in inst {
                let (_, cause) = written(w, |w, s| super::cold(w, s, c));
                assert_eq!(cause, Some(Cause::Pending(Area::Demand)));
            }
        });
    }

    // `friend_target` on another kind does not reach Kiln's panic.
    #[test]
    fn friend_target_of_another_kind_is_unaccounted() {
        with_walk("void f();", |w| {
            let f = named(w, "f", DeclKind::Function)[0];
            let (_, cause) = written(w, |w, s| super::friend_target(w, s, f));
            assert_eq!(cause, Some(Cause::Unaccounted("FriendTarget/kind")));
        });
    }
}
