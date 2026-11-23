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
    DeclFlags, DeclKind, DefinitionBits, EnumData, FriendTarget, NamespaceData, TemplLinkKind,
    VarStorage,
};
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{Ast, DeclId};

use super::KilnSubject;
use crate::wire;

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
    /// one Kiln made. **The one place the subject decides it**: WP-085
    /// replaces this body by a read of Kiln's declaration-extent table
    /// (Kiln `spec/ast.md` §19.1, one row per declaration the parser
    /// parsed, on with `ParseOptions.written`), keeping the tests of
    /// `tests/subject_entities.rs`. An explicit instantiation has a row
    /// there and is still no declaration of the subject (landed
    /// decision 2), so `instantiated` stays first.
    ///
    /// Until then the rule is structural (landed decision 1):
    ///
    /// 1. never an implicit declaration, the translation unit, or an
    ///    instantiation: an implicit or explicit instantiation, a member of
    ///    one, the declaration deduction makes of a specialisation before
    ///    its explicit specialisation is read (`instantiated`);
    /// 2. a declaration reached from the translation unit through member
    ///    lists (`listed`: Kiln links every declaration it parsed outside
    ///    a function body into its lexical context's member list, and
    ///    none it made, `kiln_ast/src/decl.rs:2161`);
    /// 3. a template parameter whose context (the template's scope, Kiln
    ///    `TemplateParamList`) is the translation unit or listed;
    /// 4. a declaration that is not listed is written only when it is
    ///    declared in a block (its lexical context is a function, a
    ///    deduction guide, a requires-expression, or a variable or data
    ///    member whose initialiser holds it) or in a local class or
    ///    enumeration (one that is not listed itself), and that context is
    ///    written. A class that is listed lists every member the parser
    ///    parsed, so a declaration it does not list (the one deduction
    ///    makes for `template <> void A::mt<int>(int)`, whose lexical
    ///    context is `A`) is not written.
    pub(super) fn written(&self, d: DeclId) -> bool {
        let mut d = d;
        for _ in 0..self.chain_bound() {
            let n = self.ast.decls.get(d);
            if n.has(DeclFlags::IMPLICIT) || n.kind() == DeclKind::TranslationUnit {
                return false;
            }
            if self.instantiated(d) {
                return false;
            }
            if self.listed(d) {
                return true;
            }
            let k = n.kind();
            let up = if template_parm_kind(k) { n.ctx() } else { self.ast.decls.lexical_ctx(d) };
            let Some(up) = up else { return false };
            let uk = self.kind(up);
            if template_parm_kind(k) && (uk == DeclKind::TranslationUnit || self.listed(up)) {
                return true;
            }
            let block = function_kind(uk)
                || matches!(
                    uk,
                    DeclKind::CXXDeductionGuide
                        | DeclKind::RequiresExprBody
                        | DeclKind::Var
                        | DeclKind::Field
                );
            let local_class =
                matches!(uk, DeclKind::CXXRecord | DeclKind::Enum) && !self.listed(up);
            if !block && !local_class {
                return false;
            }
            d = up;
        }
        false
    }

    /// How many steps a walk up a chain of contexts or origins may take: a
    /// chain longer than the arena has a cycle, which Kiln's AST does not
    /// make. No answer of the subject is cut short by it on a well-formed
    /// AST (landed decision 18).
    pub(super) fn chain_bound(&self) -> usize {
        self.ast.decls.len() + 2
    }

    /// Implicit, or made by instantiation: never written, never walked.
    pub(super) fn instantiated_or_implicit(&self, d: DeclId) -> bool {
        self.ast.decls.get(d).has(DeclFlags::IMPLICIT) || self.instantiated(d)
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
    /// lists, not descending into an implicit declaration (a builtin record
    /// such as `__va_list_tag`) or an instantiation (an explicit
    /// instantiation is in its context's member list; its members are
    /// Kiln's). The set is built once, on first use.
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
                    if ast.decls.get(x).has(DeclFlags::IMPLICIT) || self.instantiated(x) {
                        return;
                    }
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
        // A partial specialisation is a template ([temp.spec.partial]);
        // Kiln makes it a `CXXRecord` or a `Var` with a partial
        // specialisation link, never a `…PartialSpecialization` kind.
        if self.ast.partial_spec_info(e).is_some() {
            return wire::KIND_TEMPLATE;
        }
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
            // A concept is not counted a template (§6.2 rule 3).
            DeclKind::ClassTemplate
            | DeclKind::FunctionTemplate
            | DeclKind::VarTemplate
            | DeclKind::TypeAliasTemplate
            | DeclKind::BuiltinTemplate => wire::KIND_TEMPLATE,
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
            DeclKind::Var | DeclKind::VarTemplateSpecialization => self.is_var_definition(d),
            DeclKind::CXXRecord | DeclKind::ClassTemplateSpecialization | DeclKind::Enum => {
                ast.definition(d) == Some(d)
            }
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
        for _ in 0..self.chain_bound() {
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
        ast.decls
            .function(f)
            .map(|x| ast.lists.iter::<DeclId>(x.params).collect())
            .unwrap_or_default()
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
        for _ in 0..self.chain_bound() {
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
    /// enumeration, an anonymous union or anonymous struct (Kiln gives
    /// both the enclosing class `IndirectField`s).
    pub(super) fn is_transparent(&self, d: DeclId) -> bool {
        let ast = self.ast;
        match self.kind(d) {
            DeclKind::Namespace => ast
                .decls
                .namespace(d)
                .is_some_and(|n| n.flags & (NamespaceData::INLINE | NamespaceData::ANONYMOUS) != 0),
            DeclKind::Enum => {
                ast.decls.enumeration(d).is_some_and(|x| x.flags & EnumData::SCOPED == 0)
            }
            DeclKind::CXXRecord => {
                ast.decls.record(d).is_some_and(|r| r.defn.contains(DefinitionBits::ANONYMOUS))
            }
            _ => false,
        }
    }

    /// An unnamed class (§7.1 rule 4: "declared in an unnamed class"); an
    /// unnamed enumeration is not one (it is unscoped, so its enumerators
    /// are named without it).
    fn is_unnamed_class(&self, d: DeclId) -> bool {
        self.kind(d) == DeclKind::CXXRecord && self.identifier_of(d).is_empty()
    }

    /// Whether written declaration `d` is a friend declaration's: declared
    /// in a class (its lexical context) as a member of a namespace (its
    /// semantic context).
    fn is_friend_declaration(&self, d: DeclId) -> bool {
        let ast = self.ast;
        let n = ast.decls.get(d);
        ast.decls.lexical_ctx(d).is_some_and(|l| {
            self.kind(l) == DeclKind::CXXRecord
                && n.ctx().is_some_and(|c| c != l && !matches!(self.kind(c), DeclKind::CXXRecord))
        })
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
            || self.first_written(e).is_none()
            // A hidden friend, or a class only a friend declaration
            // declares, is not found by qualified lookup until a
            // declaration at namespace scope exists ([namespace.memdef]/3).
            || self.written_declarations(e).iter().all(|&d| self.is_friend_declaration(d));
        if not_nameable {
            return None;
        }
        let mut names = vec![id];
        let mut p = self.parent_decl(e)?;
        for _ in 0..self.chain_bound() {
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
            match (pid.is_empty(), matches!(pk, DeclKind::Namespace | DeclKind::Enum)) {
                // An unnamed namespace is skipped, and so is an unnamed
                // enumeration (always unscoped).
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
