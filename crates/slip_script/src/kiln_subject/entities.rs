//! Entities, declarations, kinds and structure over Kiln's declaration
//! arena (`spec/api.md` §5.1, §5.5, §6.2 to §6.4, §7.2 rule 2).
//!
//! **Numbering** (`subject.rs`, the contract of one number per value):
//! an entity is numbered by the `DeclId` index of the declaration that is
//! the entity ([`KilnSubject::entity`]); a declaration by the `DeclId`
//! index of the declaration as written, which for a template is the
//! template declaration and never its pattern (one written
//! `template <class T> struct S {}` is one declaration, although Kiln
//! makes two nodes of it). Both are stable for the life of the AST.

use std::cmp::Ordering;

use slip_kiln::kiln::ast::decl::{
    DeclFlags, DeclKind, DefinitionBits, EnumData, FriendTarget, NamespaceData, TagKind,
    TemplLinkKind, VarStorage,
};
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{Ast, DeclId};

use super::KilnSubject;
use crate::wire;

/// How far a chain of contexts or of origins is followed (an AST is far
/// shallower; the bound only keeps a malformed one from looping).
pub(super) const LIMIT: u32 = 4096;

/// Whether a declaration of this kind is a value of the subject: an entity
/// (§5.1, "a namespace, type, function, variable, template, parameter, or
/// another thing C++ gives a name or declares") and, when written, a
/// declaration. The others declare no entity of their own (a linkage
/// specification, `static_assert`, a friend declaration's wrapper, whose
/// befriended declaration is the value), are not entities of C++ (a
/// label), or are Kiln's resolution artefacts (a using-shadow).
pub(super) fn value_kind(k: DeclKind) -> bool {
    !matches!(
        k,
        DeclKind::LinkageSpec
            | DeclKind::Export
            | DeclKind::Empty
            | DeclKind::AccessSpec
            | DeclKind::RequiresExprBody
            | DeclKind::FileScopeAsm
            | DeclKind::UsingDirective
            | DeclKind::UsingEnum
            | DeclKind::Friend
            | DeclKind::StaticAssert
            | DeclKind::ConstevalBlock
            | DeclKind::TemplateParamObject
            | DeclKind::UsingShadow
            | DeclKind::ConstructorUsingShadow
            | DeclKind::UsingPack
            | DeclKind::UnresolvedUsingIfExists
            | DeclKind::Label
            | DeclKind::ContractResultName
            | DeclKind::LifetimeExtendedTemporary
            | DeclKind::ImplicitParam
            | DeclKind::ImplicitConceptSpecialization
            | DeclKind::FriendTemplate
    )
}

/// A function, member functions included (§6.2: `is_function`).
pub(super) fn function_kind(k: DeclKind) -> bool {
    matches!(
        k,
        DeclKind::Function
            | DeclKind::CXXMethod
            | DeclKind::CXXConstructor
            | DeclKind::CXXDestructor
            | DeclKind::CXXConversion
    )
}

/// A template parameter.
fn template_parm_kind(k: DeclKind) -> bool {
    matches!(
        k,
        DeclKind::TemplateTypeParm | DeclKind::NonTypeTemplateParm | DeclKind::TemplateTemplateParm
    )
}

/// A template declaration (Kiln's `is_template_kind`).
pub(super) fn template_kind(k: DeclKind) -> bool {
    matches!(
        k,
        DeclKind::ClassTemplate
            | DeclKind::FunctionTemplate
            | DeclKind::VarTemplate
            | DeclKind::TypeAliasTemplate
            | DeclKind::Concept
            | DeclKind::BuiltinTemplate
    )
}

/// A context that is not a scope of its own: its declarations belong to
/// the scope around it (Kiln's `redecl_context`).
fn see_through(k: DeclKind) -> bool {
    matches!(k, DeclKind::LinkageSpec | DeclKind::Export | DeclKind::RequiresExprBody)
}

impl<'u> KilnSubject<'u> {
    pub(super) fn ast(&self) -> &'u Ast {
        self.ast
    }

    /// The declaration numbered `n`, when `n` is one of the arena's.
    pub(super) fn decl(&self, n: u32) -> Option<DeclId> {
        (n >= 1 && n as usize <= self.ast.decls.len()).then(|| DeclId::new(n))
    }

    pub(super) fn kind(&self, d: DeclId) -> DeclKind {
        self.ast.decls.get(d).kind()
    }

    /// The entity a declaration is a declaration of: its first
    /// declaration, and for the pattern of a template the template. The
    /// same two lines as `slip_digest`'s `Walk::entity`
    /// (`crates/slip_digest/src/keys.rs`), which WP-095 tests agree.
    pub(super) fn entity(&self, d: DeclId) -> DeclId {
        let ast = self.ast;
        if let Some((TemplLinkKind::Pattern, i)) = ast.templ_link(d).get()
            && let Some(t) = ast.templates.info(i)
        {
            return ast.first(t.decl);
        }
        ast.first(d)
    }

    /// The declaration as written that `d` is part of: the template
    /// declaration for a template's pattern, else `d`.
    pub(super) fn as_written(&self, d: DeclId) -> DeclId {
        self.ast.described_template(d).unwrap_or(d)
    }

    /// A live entity (§5.2): a value kind, and the declaration that is
    /// its entity.
    pub(super) fn is_entity(&self, d: DeclId) -> bool {
        value_kind(self.kind(d)) && self.entity(d) == d
    }

    /// A live declaration: written, of a value kind, not a template's
    /// pattern (the template declaration stands for it), not the
    /// translation unit (the global namespace has no written declaration)
    /// and not a function parameter (§6.4 rule 3: `declarations_of` a
    /// parameter is `unsupported`, so no query yields one).
    pub(super) fn is_declaration(&self, d: DeclId) -> bool {
        let k = self.kind(d);
        value_kind(k)
            && k != DeclKind::TranslationUnit
            && k != DeclKind::ParmVar
            && self.as_written(d) == d
            && self.written(d)
    }

    // ---- written declarations --------------------------------------------

    /// Whether `d` is a declaration written in the unit (§6.4 rule 3), not
    /// one Kiln made: not implicit, not an instantiation, not a
    /// declaration of a specialisation made before its explicit
    /// specialisation was seen.
    ///
    /// Kiln links every declaration it parsed outside a function body into
    /// the member list of its lexical context, and none it made (an
    /// implicit member, an instantiation, the declaration deduction makes
    /// of `f<char>` before `template <> void f<char>(char)` is read:
    /// `kiln_ast/src/decl.rs:2161`, "does not link it into any member
    /// list"). So a declaration outside function bodies is written when it
    /// is reached from the translation unit through member lists (with a
    /// template's pattern and a friend's befriended declaration), and one
    /// inside a body (a parameter, a local entity, a template parameter)
    /// when its context is written and it is not an instantiation.
    pub(super) fn written(&self, d: DeclId) -> bool {
        let mut d = d;
        for _ in 0..LIMIT {
            let n = self.ast.decls.get(d);
            if n.has(DeclFlags::IMPLICIT) || n.kind() == DeclKind::TranslationUnit {
                return false;
            }
            if self.listed(d) {
                return true;
            }
            if self.instantiated(d) {
                return false;
            }
            let k = n.kind();
            let up = if template_parm_kind(k) { n.ctx() } else { self.ast.decls.lexical_ctx(d) };
            let Some(up) = up else { return false };
            let uk = self.kind(up);
            if template_parm_kind(k) && (uk == DeclKind::TranslationUnit || self.listed(up)) {
                return true;
            }
            // Only a function or a class or enumeration declared in one
            // (a local class, a closure type) holds declarations that are
            // not listed.
            let local_holder = function_kind(uk)
                || matches!(
                    uk,
                    DeclKind::CXXRecord
                        | DeclKind::Enum
                        | DeclKind::CXXDeductionGuide
                        | DeclKind::RequiresExprBody
                        | DeclKind::Var
                        | DeclKind::Field
                );
            if !local_holder && !template_parm_kind(k) {
                return false;
            }
            d = up;
        }
        false
    }

    /// Made by instantiation or before an explicit specialisation.
    fn instantiated(&self, d: DeclId) -> bool {
        let ast = self.ast;
        match ast.templ_link(d).get() {
            Some((TemplLinkKind::Specialization, i)) => {
                ast.templates.spec(i).is_none_or(|s| s.kind != SpecKind::ExplicitSpecialization)
            }
            Some((TemplLinkKind::MemberOfSpecialization, i)) => ast
                .templates
                .member_spec(i)
                .is_none_or(|s| s.kind != SpecKind::ExplicitSpecialization),
            _ => ast.templates.instantiated_from_map(d).is_some(),
        }
    }

    /// Whether `d` is reached from the translation unit through member
    /// lists. The set is built once, on first use.
    fn listed(&self, d: DeclId) -> bool {
        let bits = self.listed.get_or_init(|| self.build_listed());
        let i = d.index() as usize;
        bits.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1)
    }

    fn build_listed(&self) -> Vec<u64> {
        let ast = self.ast;
        let mut bits = vec![0u64; ast.decls.len() / 64 + 2];
        let set = |d: DeclId, bits: &mut Vec<u64>| -> bool {
            let i = d.index() as usize;
            let (w, b) = (i / 64, 1u64 << (i % 64));
            let fresh = bits[w] & b == 0;
            bits[w] |= b;
            fresh
        };
        let mut stack = vec![ast.tu];
        while let Some(c) = stack.pop() {
            for m in ast.decls.members(c) {
                let add = |x: DeclId, bits: &mut Vec<u64>, stack: &mut Vec<DeclId>| {
                    if set(x, bits) {
                        stack.push(x);
                        if let Some(p) = ast.template_info(x).and_then(|t| t.pattern)
                            && set(p, bits)
                        {
                            stack.push(p);
                        }
                    }
                };
                add(m, &mut bits, &mut stack);
                if self.kind(m) == DeclKind::Friend {
                    match ast.decls.friend_target(m) {
                        FriendTarget::Decl(x) => add(x, &mut bits, &mut stack),
                        FriendTarget::Type { owned_tag: Some(x), .. } => {
                            add(x, &mut bits, &mut stack);
                        }
                        FriendTarget::Type { owned_tag: None, .. } => {}
                    }
                }
            }
        }
        bits
    }

    /// The written declarations of entity `e`, in unit order.
    pub(super) fn written_declarations(&self, e: DeclId) -> Vec<DeclId> {
        let ast = self.ast;
        let mut out: Vec<DeclId> = match ast.template_info(e).and_then(|t| t.pattern) {
            // A template's redeclarations are its pattern's.
            Some(p) => ast.redecls_of(p).map(|x| self.as_written(x)).collect(),
            None => ast.redecls_of(e).collect(),
        };
        out.retain(|&d| self.is_declaration(d) && self.entity(d) == e);
        out.sort_by(|&a, &b| self.decl_order(a, b));
        out.dedup();
        out
    }

    /// The first written declaration of `e` in unit order.
    pub(super) fn first_written(&self, e: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let all: Box<dyn Iterator<Item = DeclId>> =
            match ast.template_info(e).and_then(|t| t.pattern) {
                Some(p) => Box::new(ast.redecls_of(p).map(|x| self.as_written(x))),
                None => Box::new(ast.redecls_of(e)),
            };
        all.filter(|&d| self.is_declaration(d) && self.entity(d) == e)
            .min_by(|&a, &b| self.decl_order(a, b))
    }

    /// Unit order of two declarations (§1): by first token when Kiln
    /// recorded the declaration's extent, then by the name; outermost
    /// first is the first token's order.
    pub(super) fn decl_order(&self, a: DeclId, b: DeclId) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        let ast = self.ast;
        let first = |d: DeclId| match ast.decl_extent(d) {
            Some(x) if x.full.lo.is_valid() => x.full.lo,
            _ => ast.decls.get(d).loc(),
        };
        let own = |d: DeclId| match ast.decl_extent(d) {
            Some(x) if x.own.lo.is_valid() => x.own.lo,
            _ => ast.decls.get(d).loc(),
        };
        self.order(first(a), first(b))
            .then_with(|| self.order(own(a), own(b)))
            .then_with(|| self.order(ast.decls.get(a).loc(), ast.decls.get(b).loc()))
            .then(a.cmp(&b))
    }

    /// Unit order of two entities by their first written declarations;
    /// one with none sorts last.
    pub(super) fn entity_order(&self, a: DeclId, b: DeclId) -> Ordering {
        match (self.first_written(a), self.first_written(b)) {
            (Some(x), Some(y)) => self.decl_order(x, y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.cmp(&b),
        }
    }

    // ---- kinds (§6.2) --------------------------------------------------------

    /// Kind bits 8 to 13 of entity `e` (§3.4, §6.2 rule 3).
    pub(super) fn entity_kinds(&self, e: DeclId) -> u64 {
        match self.kind(e) {
            DeclKind::TranslationUnit | DeclKind::Namespace => wire::KIND_NAMESPACE,
            DeclKind::CXXRecord
            | DeclKind::ClassTemplateSpecialization
            | DeclKind::Enum
            | DeclKind::Typedef
            | DeclKind::TypeAlias => wire::KIND_TYPE,
            k if function_kind(k) => wire::KIND_FUNCTION,
            DeclKind::Var
            | DeclKind::Field
            | DeclKind::IndirectField
            | DeclKind::VarTemplateSpecialization => wire::KIND_VARIABLE,
            // A partial specialisation is a template ([temp.spec.partial]);
            // a concept is not counted one (§6.2 rule 3).
            DeclKind::ClassTemplate
            | DeclKind::FunctionTemplate
            | DeclKind::VarTemplate
            | DeclKind::TypeAliasTemplate
            | DeclKind::BuiltinTemplate
            | DeclKind::ClassTemplatePartialSpecialization
            | DeclKind::VarTemplatePartialSpecialization => wire::KIND_TEMPLATE,
            DeclKind::ParmVar => wire::KIND_PARAMETER,
            _ => 0,
        }
    }

    /// Whether written declaration `d` is a definition ([basic.def]/2).
    pub(super) fn is_definition(&self, d: DeclId) -> bool {
        let ast = self.ast;
        let k = self.kind(d);
        match k {
            DeclKind::Namespace
            | DeclKind::NamespaceAlias
            | DeclKind::EnumConstant
            | DeclKind::Field
            | DeclKind::IndirectField
            | DeclKind::Concept
            | DeclKind::Binding => true,
            DeclKind::ClassTemplate
            | DeclKind::FunctionTemplate
            | DeclKind::VarTemplate
            | DeclKind::TypeAliasTemplate => match ast.template_info(d).and_then(|t| t.pattern) {
                Some(p) => self.is_definition(p),
                None => false,
            },
            DeclKind::Var
            | DeclKind::VarTemplateSpecialization
            | DeclKind::VarTemplatePartialSpecialization => self.is_var_definition(d),
            DeclKind::CXXRecord
            | DeclKind::ClassTemplateSpecialization
            | DeclKind::ClassTemplatePartialSpecialization
            | DeclKind::Enum => ast.definition(d) == Some(d),
            k if function_kind(k) => ast.definition(d) == Some(d),
            _ => false,
        }
    }

    /// [basic.def]/2 for a variable, as `slip_digest`'s
    /// `units::is_var_definition` decides it (Kiln's own
    /// `Sema::is_var_definition`): not a non-inline static data member
    /// declared in its class, not a block-scope `extern`, not an explicit
    /// specialisation of a static data member without an initialiser, and
    /// either initialised or not `extern`.
    fn is_var_definition(&self, d: DeclId) -> bool {
        let ast = self.ast;
        let Some(v) = ast.decls.var(d) else { return false };
        let in_class = ast.decls.lexical_ctx(d).is_some_and(|c| ast.decls.record(c).is_some());
        if in_class {
            return v.spec.is_inline();
        }
        let n = ast.decls.get(d);
        let local_extern = n.misc().local_extern()
            || (v.spec.storage() == VarStorage::Extern
                && n.ctx().is_some_and(|c| function_kind(self.kind(c))));
        if local_extern || ast.is_member_spec_declaration(d) {
            return false;
        }
        let is_extern = (ast.decls.is_single_line_linkage(d)
            || v.spec.storage() == VarStorage::Extern)
            && !ast.has_own_selectany(d);
        v.init.is_some() || !is_extern
    }

    // ---- names (§6.3) ---------------------------------------------------------

    /// The identifier `d` is declared with; empty when its name is not one.
    pub(super) fn identifier_of(&self, d: DeclId) -> Vec<u8> {
        let ast = self.ast;
        match ast.names.view(ast.decls.get(d).name()) {
            DeclNameView::Identifier(s) => ast.symbols().as_str(s).as_bytes().to_vec(),
            _ => Vec::new(),
        }
    }

    // ---- structure (§6.4, §5.5) ---------------------------------------------

    /// The scope `d` is declared in, as an entity: its semantic context
    /// with linkage specifications and `export` seen through.
    pub(super) fn scope_of(&self, d: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let mut c = ast.decls.get(d).ctx()?;
        for _ in 0..LIMIT {
            if !see_through(self.kind(c)) {
                return Some(self.entity(c));
            }
            c = ast.decls.get(c).ctx()?;
        }
        None
    }

    /// §6.4 rule 1.
    pub(super) fn parent_decl(&self, e: DeclId) -> Option<DeclId> {
        let k = self.kind(e);
        if k == DeclKind::TranslationUnit {
            return None;
        }
        if template_parm_kind(k)
            && let Some(t) = self.template_parm_owner(e)
        {
            return Some(t);
        }
        let p = self.scope_of(e)?;
        self.is_entity(p).then_some(p)
    }

    /// The template whose template-head declares template parameter `p`
    /// (Kiln gives a template parameter the template's context, not the
    /// template, as `ctx`: `kiln_ast/src/templates.rs` `TemplateParamList`).
    /// The map is built once, on first use.
    fn template_parm_owner(&self, p: DeclId) -> Option<DeclId> {
        let map = self.parm_owner.get_or_init(|| {
            let ast = self.ast;
            let mut map = std::collections::HashMap::new();
            for d in ast.decls.ids() {
                let lists: Vec<_> = if let Some(t) = ast.template_info(d) {
                    vec![t.params]
                } else if let Some((_, pi)) = ast.partial_spec_info(d) {
                    vec![pi.params]
                } else {
                    continue;
                };
                for l in lists {
                    for x in ast.lists.iter::<DeclId>(ast.templates.param_list(l).params) {
                        map.entry(x.index()).or_insert(self.entity(d).index());
                    }
                }
            }
            map
        });
        map.get(&p.index()).map(|&t| DeclId::new(t))
    }

    /// The function declaration whose parameters `parameters_of` gives for
    /// `d` (a function, a function template, or a declaration of either).
    pub(super) fn function_of(&self, d: DeclId) -> Option<DeclId> {
        let k = self.kind(d);
        if function_kind(k) {
            return Some(d);
        }
        if k == DeclKind::FunctionTemplate {
            return self.ast.template_info(d).and_then(|t| t.pattern);
        }
        None
    }

    /// The parameters of function declaration `f`, in order.
    pub(super) fn parameters_of(&self, f: DeclId) -> Vec<DeclId> {
        let ast = self.ast;
        ast.decls.function(f).map(|x| ast.lists.iter::<DeclId>(x.params).collect()).unwrap_or_default()
    }

    /// §5.5: the written entity `e` comes from. A written entity is its own
    /// origin; a specialisation's is its template's; a member of an
    /// instantiation's is the member it was instantiated from; followed
    /// until a written entity (the member specialisation of a member
    /// template: the specialisation, its template in the instantiation,
    /// the member template of the class template). An entity Kiln made
    /// that comes from nothing (an implicit member) is its own origin.
    pub(super) fn origin_decl(&self, e: DeclId) -> DeclId {
        let ast = self.ast;
        let mut x = e;
        for _ in 0..LIMIT {
            if self.first_written(x).is_some() {
                return x;
            }
            let next = match ast.spec_info(x) {
                Some((_, s)) => Some(s.template),
                None => ast.instantiated_from(x),
            };
            match next.map(|n| self.entity(n)) {
                Some(n) if n != x && self.is_entity(n) => x = n,
                _ => return x,
            }
        }
        x
    }

    /// Whether `e` is a scope `lookup` walks into (§6.1 rule 4).
    pub(super) fn is_scope(&self, e: DeclId) -> bool {
        matches!(
            self.kind(e),
            DeclKind::TranslationUnit
                | DeclKind::Namespace
                | DeclKind::CXXRecord
                | DeclKind::Enum
                | DeclKind::ClassTemplate
        )
    }

    /// Whether the members of `d` are members of the scope around it for
    /// `lookup` (§6.1 rule 3): an inline or unnamed namespace, an unscoped
    /// enumeration, an anonymous union.
    pub(super) fn is_transparent(&self, d: DeclId) -> bool {
        let ast = self.ast;
        match self.kind(d) {
            DeclKind::Namespace => ast.decls.namespace(d).is_some_and(|n| {
                n.flags & (NamespaceData::INLINE | NamespaceData::ANONYMOUS) != 0
            }),
            DeclKind::Enum => ast.decls.enumeration(d).is_some_and(|x| x.flags & EnumData::SCOPED == 0),
            DeclKind::CXXRecord => self.is_anonymous_union(d),
            _ => false,
        }
    }

    fn is_anonymous_union(&self, d: DeclId) -> bool {
        self.ast.decls.record(d).is_some_and(|r| {
            r.tag == TagKind::Union && r.defn.contains(DefinitionBits::ANONYMOUS)
        })
    }

    /// An unnamed class or enumeration, or anything else with no
    /// identifier that is not a namespace (§7.1 rule 4: "declared in an
    /// unnamed class").
    fn is_unnamed_class(&self, d: DeclId) -> bool {
        matches!(self.kind(d), DeclKind::CXXRecord | DeclKind::Enum) && self.identifier_of(d).is_empty()
    }

    /// §7.2 rule 2 and §7.1 rule 4: the identifiers from the outermost
    /// named scope to `e`, or `None` when `e` cannot be named by a
    /// qualified name without template arguments.
    pub(super) fn qualified_decl(&self, e: DeclId) -> Option<Vec<Vec<u8>>> {
        let ast = self.ast;
        let k = self.kind(e);
        let id = self.identifier_of(e);
        let not_nameable = id.is_empty()
            || k == DeclKind::ParmVar
            || template_parm_kind(k)
            || matches!(
                k,
                DeclKind::Using
                    | DeclKind::UnresolvedUsingValue
                    | DeclKind::UnresolvedUsingTypename
                    | DeclKind::CXXDeductionGuide
                    | DeclKind::Binding
            )
            || ast.spec_info(e).is_some()
            || ast.partial_spec_info(e).is_some()
            || self.first_written(e).is_none();
        if not_nameable {
            return None;
        }
        let mut names = vec![id];
        let mut p = self.parent_decl(e)?;
        for _ in 0..LIMIT {
            let pk = self.kind(p);
            if pk == DeclKind::TranslationUnit {
                names.reverse();
                return Some(names);
            }
            let local_or_templated = function_kind(pk)
                || template_kind(pk)
                || ast.spec_info(p).is_some()
                || ast.partial_spec_info(p).is_some()
                || ast.member_spec_info(p).is_some()
                || self.first_written(p).is_none()
                || self.is_unnamed_class(p);
            if local_or_templated {
                return None;
            }
            let pid = self.identifier_of(p);
            match (pid.is_empty(), pk == DeclKind::Namespace) {
                // An unnamed namespace is skipped.
                (true, true) => {}
                (true, false) => return None,
                (false, _) => names.push(pid),
            }
            p = self.parent_decl(p)?;
        }
        None
    }

    /// The declaration of a friend's wrapper or of a member, as the value
    /// it declares, for the walks of `lookup`.
    pub(super) fn befriended(&self, m: DeclId) -> Option<DeclId> {
        match self.ast.decls.friend_target(m) {
            FriendTarget::Decl(x) => Some(x),
            FriendTarget::Type { owned_tag, .. } => owned_tag,
        }
    }
}
