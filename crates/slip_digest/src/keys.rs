//! Keys (`spec/digest.md` §3, `spec/digest-kinds.md` §3, §5.2): which
//! key each declaration has, its parent, name and discriminator, mapped
//! under the walk's rules.

use slip_kiln::kiln::ast::decl::{
    DeclKind, FnStorage, FriendKind, LinkageLang, NamespaceData, TemplLinkKind, VarStorage,
};
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::ast::types::TypeView;
use slip_kiln::kiln::ast::{DeclId, QualType};
use slip_model::Digest;
use slip_model::keys::{DISC_TAG, Key, KeyKind, Name, Parent};

use crate::Cause;
use crate::stream::{FlagWord, Stream};
use crate::tables::{overloaded_operator, ref_qual};
use crate::walk::{TOO_DEEP, Walk};

/// What a declaration kind is in the walk (`spec/digest-kinds.md` §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// Declares nothing the digest writes.
    Nothing,
    /// A scope whose declarations belong to the scope around it.
    Transparent,
    /// The translation unit: `Parent::root`.
    Root,
    /// A unit with a name: the key kind.
    Entity(KeyKind),
    /// A unit keyed by content (`spec/digest.md` §3.3).
    Content(KeyKind),
    /// Numbered inside the unit that holds it (`spec/digest.md` §4.3).
    Local,
    /// Written inside another declaration's stream.
    InParent,
    /// `unsupported`: no construction site in Kiln.
    Unsupported(&'static str),
}

/// The facts of one `DeclKind` the walk dispatches on, in one place so
/// that every predicate over declaration kinds is one exhaustive match.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools, reason = "independent facts of a kind, read one at a time")]
pub(crate) struct KindInfo {
    pub role: Role,
    /// A unit only when some stream refers to it (digest-kinds §5.2).
    pub on_demand: bool,
    /// Has the function discriminator (digest-kinds §3.3).
    pub function: bool,
    /// A declaration whose scope makes what it holds local (a function
    /// body, an initialiser, a requires-expression's parameters).
    pub local_scope: bool,
    /// May have `Parent::c_linkage` (digest-kinds §5.2).
    pub c_linkage: bool,
}

const fn k(role: Role) -> KindInfo {
    KindInfo { role, on_demand: false, function: false, local_scope: false, c_linkage: false }
}

const fn func(role: Role) -> KindInfo {
    KindInfo { role, on_demand: false, function: true, local_scope: true, c_linkage: false }
}

/// The table of `spec/digest-kinds.md` §5.2 as a match.
#[allow(clippy::too_many_lines, reason = "one arm per DeclKind")]
pub(crate) const fn info(kind: DeclKind) -> KindInfo {
    use KeyKind as K;
    use Role::{Content, Entity, InParent, Local, Nothing, Transparent, Unsupported};
    match kind {
        DeclKind::TranslationUnit => k(Role::Root),
        DeclKind::Namespace => k(Entity(K::Namespace)),
        DeclKind::NamespaceAlias => k(Entity(K::NamespaceAlias)),
        DeclKind::LinkageSpec | DeclKind::Export => k(Transparent),
        DeclKind::Empty | DeclKind::AccessSpec => k(Nothing),
        DeclKind::RequiresExprBody => KindInfo { local_scope: true, ..k(Nothing) },
        DeclKind::FileScopeAsm => k(Content(K::FileScopeAsm)),
        DeclKind::UsingDirective => k(Content(K::UsingDirective)),
        DeclKind::UsingEnum => k(Content(K::UsingEnum)),
        DeclKind::Friend => k(Content(K::Friend)),
        DeclKind::StaticAssert => k(Content(K::StaticAssert)),
        DeclKind::ConstevalBlock => k(Content(K::ConstevalBlock)),
        DeclKind::TemplateParamObject => {
            KindInfo { on_demand: true, ..k(Content(K::TemplateParamObject)) }
        }
        DeclKind::Using | DeclKind::UnresolvedUsingValue | DeclKind::UnresolvedUsingTypename => {
            k(Entity(K::UsingDeclaration))
        }
        DeclKind::UsingShadow | DeclKind::ConstructorUsingShadow => k(Entity(K::UsingShadow)),
        DeclKind::UnresolvedUsingIfExists => k(Entity(K::UnresolvedIfExists)),
        DeclKind::Typedef | DeclKind::TypeAlias => k(Entity(K::Alias)),
        DeclKind::Enum => k(Entity(K::Enumeration)),
        DeclKind::EnumConstant => k(Entity(K::Enumerator)),
        DeclKind::CXXRecord => k(Entity(K::Class)),
        DeclKind::ClassTemplateSpecialization => k(Entity(K::Specialisation)),
        DeclKind::VarTemplateSpecialization => {
            KindInfo { local_scope: true, ..k(Entity(K::Specialisation)) }
        }
        DeclKind::ClassTemplatePartialSpecialization => k(Entity(K::PartialSpecialisation)),
        DeclKind::VarTemplatePartialSpecialization => {
            KindInfo { local_scope: true, ..k(Entity(K::PartialSpecialisation)) }
        }
        DeclKind::Field => KindInfo { local_scope: true, ..k(Entity(K::Field)) },
        DeclKind::IndirectField => k(Entity(K::Field)),
        DeclKind::Function => KindInfo { c_linkage: true, ..func(Entity(K::Function)) },
        DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion => func(Entity(K::Function)),
        DeclKind::CXXDeductionGuide => {
            KindInfo { function: false, ..func(Entity(K::DeductionGuide)) }
        }
        DeclKind::Var => KindInfo { local_scope: true, c_linkage: true, ..k(Entity(K::Variable)) },
        DeclKind::Binding => k(Entity(K::Binding)),
        DeclKind::ParmVar => KindInfo { local_scope: true, ..k(Local) },
        DeclKind::Label | DeclKind::ContractResultName => k(Local),
        DeclKind::ClassTemplate => k(Entity(K::ClassTemplate)),
        DeclKind::FunctionTemplate => k(Entity(K::FunctionTemplate)),
        DeclKind::VarTemplate => k(Entity(K::VariableTemplate)),
        DeclKind::TypeAliasTemplate => k(Entity(K::AliasTemplate)),
        DeclKind::Concept => KindInfo { local_scope: true, ..k(Entity(K::Concept)) },
        DeclKind::BuiltinTemplate => KindInfo { on_demand: true, ..k(Entity(K::ClassTemplate)) },
        DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm
        | DeclKind::LifetimeExtendedTemporary => k(InParent),
        DeclKind::UsingPack => k(Unsupported("DeclKind/UsingPack")),
        DeclKind::FriendTemplate => k(Unsupported("DeclKind/FriendTemplate")),
        DeclKind::ImplicitParam => k(Unsupported("DeclKind/ImplicitParam")),
        DeclKind::ImplicitConceptSpecialization => {
            k(Unsupported("DeclKind/ImplicitConceptSpecialization"))
        }
    }
}

/// An entity's key as the walk computed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyRes {
    /// The mapped `KeyId`, its kind, and the cause its discriminator
    /// carries (a pending area): a stream that refers to the entity
    /// absorbs it.
    Key { id: Digest, kind: KeyKind, cause: Option<Cause> },
    /// No key: a local entity, a kind without one, or one that cannot be
    /// computed (the cause).
    None(Option<Cause>),
}

/// The cause of an entity that refers to itself through its own key.
const KEY_CYCLE: Cause = Cause::Unaccounted("key/cycle");
/// A rule that matches a member of an instantiation, not its pattern's
/// (`spec/digest.md` §3.4).
const RULE_ON_INSTANTIATED_MEMBER: Cause = Cause::Unaccounted("key/rule-on-instantiated-member");
/// A key under a table that holds a refused rule (`spec/verify.md` §2.1).
const REFUSED_TABLE: Cause = Cause::Unaccounted("key/refused-rule");
/// A member instantiated from two patterns whose mapped names differ
/// (`spec/digest.md` §3.4).
const PATTERNS_DISAGREE: Cause = Cause::Unaccounted("key/patterns-disagree");
/// A scope no row of §5.2 says can hold an entity.
const BAD_PARENT: Cause = Cause::Unaccounted("key/parent");
/// A name form no rule of §3.2 or digest §3.3 gives.
const BAD_NAME: Cause = Cause::Unaccounted("key/name");
/// How far a chain of contexts is followed (an AST is far shallower).
const CTX_LIMIT: u32 = 4096;
/// A local entity that is not a local type (a block-scope typedef or
/// using-declaration, a local variable asked for its key): it has none.
const LOCAL_ENTITY: Cause = Cause::Unaccounted("key/local");

impl Walk<'_> {
    /// The entity a declaration is a declaration of: its first
    /// declaration, and for the pattern of a template the template.
    pub(crate) fn entity(&self, d: DeclId) -> DeclId {
        let ast = self.ast;
        if let Some((TemplLinkKind::Pattern, i)) = ast.templ_link(d).get()
            && let Some(t) = ast.templates.info(i)
        {
            return ast.first(t.decl);
        }
        ast.first(d)
    }

    /// `d` is inside a function body, an initialiser or another scope
    /// whose declarations are local (`spec/digest.md` §4.3), or is a
    /// closure type.
    pub(crate) fn is_local(&self, d: DeclId) -> bool {
        let ast = self.ast;
        if ast.decls.record(d).is_some_and(|r| r.lambda.is_some()) {
            return true;
        }
        let mut c = ast.decls.get(d).ctx();
        for _ in 0..CTX_LIMIT {
            let Some(x) = c else { return false };
            let i = info(ast.decls.get(x).kind());
            if i.local_scope || ast.decls.record(x).is_some_and(|r| r.lambda.is_some()) {
                return true;
            }
            if i.role == Role::Root {
                return false;
            }
            c = ast.decls.get(x).ctx();
        }
        false
    }

    /// A block-scope declaration of a function or of a variable with
    /// linkage (`DeclMisc` `0-local-extern`): it belongs to the
    /// namespace (digest-kinds §2.4).
    ///
    /// Kiln does not set the bit yet, so the walk also reads it off the
    /// declaration the way Kiln's mangler does (`kiln_abi::mangle::
    /// local_extern_ns`): a function, or a variable declared `extern`,
    /// whose semantic context is a function (`spec/landed/digest/WP-024.md`).
    pub(crate) fn is_local_extern(&self, d: DeclId) -> bool {
        let ast = self.ast;
        let n = ast.decls.get(d);
        if n.misc().local_extern() {
            return true;
        }
        let kind = n.kind();
        let candidate = kind == DeclKind::Function
            || (kind == DeclKind::Var
                && ast.decls.var(d).is_some_and(|v| v.spec.storage() == VarStorage::Extern));
        candidate && n.ctx().is_some_and(|c| info(ast.decls.get(c).kind()).function)
    }

    /// The key of `d`'s entity, mapped (`spec/digest.md` §3.4).
    pub(crate) fn key(&mut self, d: DeclId) -> KeyRes {
        self.key_in(d, None)
    }

    /// The key of `d`'s entity, mapped; for a namespace seen from a member
    /// whose ownership is not the namespace's (`view`, rule 2 of
    /// `spec/verify.md` §2.1, `namespace_parent`), mapped in that view:
    /// rules along the chain if `view` is `Some(true)`, none if
    /// `Some(false)`. `None` is the entity's own key, mapped if it is
    /// owned (rule 1).
    fn key_in(&mut self, d: DeclId, view: Option<bool>) -> KeyRes {
        // Numbering an owner reads no key (`local.rs`).
        if crate::local::numbering_mode(self) {
            return KeyRes::None(Some(crate::local::IN_NUMBERING));
        }
        let e = self.entity(d);
        let memo = match view {
            None => self.keys.get(&e),
            Some(b) => self.owned_state.views.get(&(e, b)),
        };
        if let Some(k) = memo {
            return *k;
        }
        if self.keys_in_progress.contains(&e) {
            // What is computed above this point depends on the path that
            // reached the cycle: not memoised, as for a depth abort.
            self.aborts += 1;
            return KeyRes::None(Some(KEY_CYCLE));
        }
        if !self.enter() {
            return KeyRes::None(Some(TOO_DEEP));
        }
        let aborts = self.aborts;
        let rooted = self.local.self_rooted;
        self.keys_in_progress.insert(e);
        // A key does not depend on the unit that asks for it: the unit's
        // local state is put aside (`Walk::in_key`).
        let saved = self.take_unit();
        let types = self.take_types();
        self.in_key += 1;
        // Read from the representative: Kiln's first declaration may be
        // one an instantiation made outside its class, whose template head
        // is the instantiation's (WP-032).
        let computed = self.compute_key_in(self.representative(e), view);
        self.in_key -= 1;
        self.restore_types(types);
        self.unit = saved;
        let res = match computed {
            Ok((key, cause)) => {
                let kind = key.kind;
                let key = self.map_key(e, view, key);
                self.bytes += key.to_bytes().len() as u64;
                KeyRes::Key { id: key.id(), kind, cause }
            }
            Err(cause) => KeyRes::None(cause),
        };
        self.keys_in_progress.remove(&e);
        self.leave();
        if self.aborts == aborts && self.local.self_rooted == rooted {
            match view {
                None => self.keys.insert(e, res),
                Some(b) => self.owned_state.views.insert((e, b), res),
            };
        }
        res
    }

    /// `key` of `d`'s entity under the rules if one may apply to it
    /// (rule 1: the entity is owned, or `view` says so), else as it is.
    /// Every place that applies the rules to an entity's key calls this.
    pub(crate) fn map_key(&mut self, d: DeclId, view: Option<bool>, key: Key) -> Key {
        if self.rule_applies(d, view) { self.rules.map(key) } else { key }
    }

    /// The bytes of the mapped key of `d`'s entity, which `canonical_decl`
    /// orders by (WP-032 rule 3); `None` when it has no key. Not memoised:
    /// it is asked for once per declaration of an entity declared only by
    /// instantiations.
    pub(crate) fn key_bytes(&mut self, d: DeclId) -> Option<Vec<u8>> {
        if !matches!(self.key(d), KeyRes::Key { .. }) {
            return None;
        }
        let e = self.entity(d);
        if !self.enter() {
            return None;
        }
        let (saved, bytes) = (self.take_unit(), self.bytes);
        let types = self.take_types();
        self.in_key += 1;
        let computed = self.compute_key(self.representative(e));
        self.in_key -= 1;
        self.restore_types(types);
        self.unit = saved;
        self.bytes = bytes;
        self.leave();
        computed.ok().map(|(key, _)| self.map_key(e, None, key).to_bytes())
    }

    /// `key`: the mapped `KeyId` as a `digest` (digest-kinds §1.3).
    #[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
    pub(crate) fn write_key(&mut self, s: &mut Stream, d: DeclId) {
        match self.key(d) {
            KeyRes::Key { id, cause, .. } => {
                s.digest(&id).absorb(cause);
            }
            KeyRes::None(cause) => {
                s.digest(&Digest([0; 32]));
                s.indeterminate(cause.unwrap_or(Cause::Unaccounted("key/none")));
            }
        }
    }

    /// `name`: a declaration's name as the `Name` of digest-kinds §3.2.
    #[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
    pub(crate) fn write_name(&mut self, s: &mut Stream, d: DeclId) {
        match self.plain_name(d) {
            Ok(Some((name, cause))) => {
                s.name(&name).absorb(cause);
            }
            Ok(None) | Err(_) => {
                s.name(&Name::Anonymous).indeterminate(BAD_NAME);
            }
        }
    }

    /// `sym`: a symbol's spelling.
    pub(crate) fn write_sym(
        &mut self,
        s: &mut Stream,
        sym: slip_kiln::kiln::source::symbol::Symbol,
    ) {
        s.bytes(self.ast.symbols().as_str(sym).as_bytes());
    }

    /// The key of entity `e` before mapping, with the cause its
    /// discriminator carries; `Err` when it has none.
    pub(crate) fn compute_key(&mut self, e: DeclId) -> Result<(Key, Option<Cause>), Option<Cause>> {
        self.compute_key_in(e, None)
    }

    /// [`Walk::compute_key`], with the parent of a namespace taken in
    /// `view` (see [`Walk::key_in`]).
    fn compute_key_in(
        &mut self,
        e: DeclId,
        view: Option<bool>,
    ) -> Result<(Key, Option<Cause>), Option<Cause>> {
        let ast = self.ast;
        let n = ast.decls.get(e);
        let kind_info = info(n.kind());
        // A table with a rule `spec/verify.md` §2.1 refuses keys nothing
        // (`RuleTable::refused`): every unit is indeterminate under it.
        if self.rules.refused().is_some()
            && matches!(kind_info.role, Role::Entity(_) | Role::Content(_))
        {
            return Err(Some(REFUSED_TABLE));
        }
        match ast.templ_link(e).get() {
            Some((TemplLinkKind::Specialization, _)) => {
                if let Some((_, si)) = ast.spec_info(e) {
                    let (template, args) = (si.template, si.args);
                    return self.spec_key(template, None, args);
                }
            }
            Some((TemplLinkKind::PartialSpecialization, _)) => {
                if let Some((_, pi)) = ast.partial_spec_info(e) {
                    let (template, params, args) = (pi.template, pi.params, pi.canonical);
                    return self.spec_key(template, Some(params), args);
                }
            }
            Some((TemplLinkKind::Pattern | TemplLinkKind::MemberOfSpecialization, _)) | None => {}
        }
        let kind = match kind_info.role {
            Role::Entity(kind) => kind,
            Role::Content(kind) => {
                let (parent, parent_cause) = self.parent_in(e, view)?;
                let h = self.content_hash(e);
                let key = Key { kind, parent, name: Name::Content, disc: Some(h.digest) };
                return Ok((key, merge(parent_cause, h.cause)));
            }
            Role::Unsupported(what) => return Err(Some(Cause::Unsupported(what))),
            Role::Nothing | Role::Transparent | Role::Root | Role::Local | Role::InParent => {
                return Err(None);
            }
        };
        let decomposition = kind == KeyKind::Variable && ast.decls.is_decomposition(e);
        let kind = if decomposition { KeyKind::Decomposition } else { kind };
        let (parent, parent_cause) = self.parent_in(e, view)?;
        let mut cause;
        let name = if decomposition {
            let first =
                ast.decls.decomp_bindings(e).and_then(|l| ast.lists.iter::<DeclId>(l).next());
            match first.map(|b| (b, self.plain_name(b))) {
                // The first binding's name as the rules map it (`spec/digest.md`
                // §3.4, WP-103): a rule that renames the binding renames the
                // decomposition with it.
                Some((b, Ok(Some((name, c))))) => {
                    cause = c;
                    self.mapped_name(b).unwrap_or(name)
                }
                Some((_, Ok(None) | Err(_))) | None => return Err(Some(BAD_NAME)),
            }
        } else {
            let (name, c) = self.entity_name(e)?;
            cause = c;
            name
        };
        if let Err(what) = name.validate() {
            return Err(Some(Cause::Unaccounted(what)));
        }
        cause = merge(cause, parent_cause);
        let disc = self.disc(e, kind, parent == Parent::CLinkage);
        let disc = match disc {
            Some(h) => {
                cause = merge(cause, h.1);
                Some(h.0)
            }
            None => None,
        };
        let key = Key { kind, parent, name, disc };
        if !decomposition && !self.rules.is_empty() {
            let patterns = self.instantiated_patterns(e);
            if !patterns.is_empty() {
                return self.follow_patterns(e, &patterns, key, cause);
            }
        }
        Ok((key, cause))
    }

    /// The members of patterns that `e`'s entity was instantiated from,
    /// through Kiln's member link or its sparse map (`instantiated_from`),
    /// as entities, each once: `e` is a member of a specialisation, or of
    /// an instantiated member template. Every declaration of the entity's
    /// chain is asked, not the representative only: a friend defined in a
    /// class template and declared again at namespace scope has the
    /// namespace-scope declaration as its representative, which no
    /// instantiation made (WP-103 round 3). A friend pattern with no
    /// definition (a friend declaration of a function defined elsewhere)
    /// is not followed: the function it names is defined by its own declaration,
    /// and a rule renames it directly
    /// (`kinds/function-first-declared-as-friend`). A specialisation's own
    /// link is `spec_key`'s.
    fn instantiated_patterns(&self, e: DeclId) -> Vec<DeclId> {
        let ast = self.ast;
        let own = self.entity(e);
        let mut out: Vec<DeclId> = Vec::new();
        for d in self.chain(own) {
            let from = match ast.templ_link(d).get() {
                Some((TemplLinkKind::Specialization | TemplLinkKind::PartialSpecialization, _)) => {
                    None
                }
                Some((TemplLinkKind::Pattern | TemplLinkKind::MemberOfSpecialization, _))
                | None => ast.instantiated_from(d),
            };
            if let Some(p) = from.map(|p| self.entity(p)).filter(|&p| p != own)
                && (ast.decls.get(p).misc().friend() == FriendKind::None
                    || ast.definition(p).is_some())
                && !out.contains(&p)
            {
                out.push(p);
            }
        }
        out
    }

    /// The key of a member instantiated from `patterns` (`spec/digest.md`
    /// §3.4, WP-103): its name is the pattern member's as the rules map it,
    /// through the whole chain of patterns, so that a rule on a member of a
    /// class template reaches the members of its instantiations. Two
    /// patterns (a function befriended, and defined or declared, by two
    /// class templates, each with a definition, are not one program: one
    /// definition is the rule) whose mapped names differ leave no name to take:
    /// the key is indeterminate (`key/patterns-disagree`). A rule that
    /// matches the member itself, under either name, is not one the source
    /// can carry out (an instantiation's member is renamed only by renaming
    /// the pattern's): the key is indeterminate.
    ///
    /// Under ownership (WP-110) the name is the pattern's as its own
    /// ownership maps it (`mapped_name`), and the check of a rule on the
    /// member is made only for an owned member: a rule matches no
    /// dependency entity (rule 1).
    fn follow_patterns(
        &mut self,
        e: DeclId,
        patterns: &[DeclId],
        key: Key,
        cause: Option<Cause>,
    ) -> Result<(Key, Option<Cause>), Option<Cause>> {
        let mut name: Option<Name> = None;
        for &p in patterns {
            let Some(n) = self.mapped_name(p) else { continue };
            match &name {
                Some(prev) if *prev != n => return Err(Some(PATTERNS_DISAGREE)),
                Some(_) => {}
                None => name = Some(n),
            }
        }
        let Some(name) = name else {
            return Ok((key, cause));
        };
        let followed = Key { name, ..key.clone() };
        if self.rule_applies(e, None)
            && (self.rules.map(key.clone()) != key || self.rules.map(followed.clone()) != followed)
        {
            return Err(Some(RULE_ON_INSTANTIATED_MEMBER));
        }
        Ok((followed, cause))
    }

    /// The key of a specialisation (`params` absent) or of a partial
    /// specialisation of `template`.
    fn spec_key(
        &mut self,
        template: DeclId,
        params: Option<slip_kiln::kiln::ast::TemplateParamListId>,
        args: slip_kiln::kiln::ast::ArgListId,
    ) -> Result<(Key, Option<Cause>), Option<Cause>> {
        let (parent, template_cause) = match self.key(template) {
            KeyRes::Key { id, cause, .. } => (Parent::Unit(id), cause),
            KeyRes::None(c) => return Err(Some(c.unwrap_or(BAD_PARENT))),
        };
        let (name, mut cause) = self.entity_name(self.entity(template))?;
        // The name is the template's as the rules map it, as the parent
        // is (`spec/digest.md` §3.4, WP-103): a rule that renames a
        // template renames its specialisations with it.
        let name = self.mapped_name(template).unwrap_or(name);
        cause = merge(cause, template_cause);
        let mut s = Stream::new();
        let kind = if let Some(p) = params {
            crate::templ::template_head(self, &mut s, p);
            KeyKind::PartialSpecialisation
        } else {
            KeyKind::Specialisation
        };
        let canonical = self.ast.types.canonical_args(args);
        self.write_args(&mut s, canonical);
        let h = s.finish(DISC_TAG);
        let h = self.hashed(h);
        cause = merge(cause, h.cause);
        Ok((Key { kind, parent, name, disc: Some(h.digest) }, cause))
    }

    /// The name of `d`'s entity in its mapped key: `None` under no rule
    /// (the plain name), and where the key cannot be formed (the caller
    /// has the error already). Mapped as the entity's own key is
    /// (`map_key`): a dependency entity's name is its plain name.
    fn mapped_name(&mut self, d: DeclId) -> Option<Name> {
        if self.rules.is_empty() {
            return None;
        }
        let e = self.entity(d);
        let (key, _) = self.compute_key(self.representative(e)).ok()?;
        Some(self.map_key(e, None, key).name)
    }

    /// The parent of entity `e` (`spec/digest.md` §3), with the cause its
    /// key carries: `Err` for a local entity, whose key is `local(n)` and
    /// needs the statement walk.
    ///
    /// An enclosing namespace is keyed in `view` (`None`: in `e`'s own;
    /// rule 2, [`Walk::namespace_parent`]).
    fn parent_in(
        &mut self,
        e: DeclId,
        view: Option<bool>,
    ) -> Result<(Parent, Option<Cause>), Option<Cause>> {
        let ast = self.ast;
        let n = ast.decls.get(e);
        if ast.decls.builtin_id(e).is_some() || n.kind() == DeclKind::BuiltinTemplate {
            return Ok((Parent::Builtin, None));
        }
        if ast.decls.record(e).is_some_and(|r| r.lambda.is_some()) {
            // A closure type: `local(n)` under its owner (digest §4.3).
            return crate::local::local_parent(self, e);
        }
        if let Some(class) = self.friend_class(e) {
            return self.unit_parent(class);
        }
        let local_extern = self.is_local_extern(e);
        if info(n.kind()).c_linkage && self.in_c_linkage(e) {
            return Ok((Parent::CLinkage, None));
        }
        let mut c = n.ctx();
        let mut to_namespace = false;
        for _ in 0..CTX_LIMIT {
            let Some(x) = c else { return Ok((Parent::Root, None)) };
            let xn = ast.decls.get(x);
            let xi = info(xn.kind());
            if to_namespace {
                match xi.role {
                    Role::Root => return Ok((Parent::Root, None)),
                    Role::Entity(KeyKind::Namespace) => return self.namespace_parent(x, e, view),
                    Role::Nothing
                    | Role::Transparent
                    | Role::Entity(_)
                    | Role::Content(_)
                    | Role::Local
                    | Role::InParent
                    | Role::Unsupported(_) => {
                        c = xn.ctx();
                        continue;
                    }
                }
            }
            if xi.local_scope || ast.decls.record(x).is_some_and(|r| r.lambda.is_some()) {
                if local_extern {
                    to_namespace = true;
                    c = xn.ctx();
                    continue;
                }
                // A member of a closure type is keyed under it; a local
                // class or enumeration under its owner; any other local
                // entity has no key (digest §4.3).
                if ast.decls.record(x).is_some_and(|r| r.lambda.is_some()) {
                    return self.unit_parent(x);
                }
                if crate::local::is_local_type(self, e) {
                    return crate::local::local_parent(self, e);
                }
                return Err(Some(LOCAL_ENTITY));
            }
            match xi.role {
                Role::Root => return Ok((Parent::Root, None)),
                Role::Transparent => c = xn.ctx(),
                Role::Entity(KeyKind::Namespace) => return self.namespace_parent(x, e, view),
                Role::Entity(
                    KeyKind::Class
                    | KeyKind::Enumeration
                    | KeyKind::Specialisation
                    | KeyKind::PartialSpecialisation
                    | KeyKind::ClassTemplate,
                ) => return self.unit_parent(x),
                Role::Entity(_)
                | Role::Nothing
                | Role::Content(_)
                | Role::Local
                | Role::InParent
                | Role::Unsupported(_) => return Err(Some(BAD_PARENT)),
            }
        }
        Err(Some(BAD_PARENT))
    }

    /// The class a friend function or function template `e` is keyed in,
    /// though its semantic context is a namespace (WP-032 rule 3):
    ///
    /// - one declared by an instantiation outside its class that is
    ///   member-like ([temp.friend]/9): the class specialisation
    ///   `TemplateTables` `member_like_friend` names, or, for a
    ///   non-template friend with a requires-clause, the class
    ///   specialisation that declares it (Kiln's mangler,
    ///   `kiln_abi::mangle::constrained_friend_class`);
    /// - one every declaration of which is in a templated class, where
    ///   Kiln makes it an entity of its own that redeclares nothing
    ///   (`kiln_sema/src/decl.rs:5465`): that class
    ///   (`spec/landed/digest/WP-032.md`).
    ///
    /// `None` for any other declaration, which keeps its namespace.
    pub(crate) fn friend_class(&self, e: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let n = ast.decls.get(e);
        let kind = n.kind();
        let function = info(kind).function;
        if !(function || kind == DeclKind::FunctionTemplate) {
            return None;
        }
        if n.misc().friend() == FriendKind::None {
            return None;
        }
        let lex = ast.decls.lexical_ctx(e)?;
        if n.ctx() == Some(lex) || ast.decls.record(lex).is_none() {
            return None;
        }
        if crate::units::instantiated_outside_class(self, e) {
            if let Some(&class) = ast.templates.member_like_friend.get(e) {
                return Some(class);
            }
            let constrained = function
                && ast.templates.trailing_requires.contains(e)
                && crate::units::outside_only(self, e);
            return constrained.then_some(lex);
        }
        let in_pattern = self.redecls(e).iter().all(|&d| {
            ast.decls.get(d).ctx() != ast.decls.lexical_ctx(d)
                && self.in_templated_class(ast.decls.lexical_ctx(d))
        });
        in_pattern.then_some(lex)
    }

    /// `c` or a context lexically around it is a templated class: a class
    /// template's pattern or a partial specialisation (Kiln's
    /// `Sema::in_templated_class`), searched up to the nearest namespace.
    fn in_templated_class(&self, mut c: Option<DeclId>) -> bool {
        let ast = self.ast;
        for _ in 0..CTX_LIMIT {
            let Some(x) = c else { return false };
            let kind = ast.decls.get(x).kind();
            if kind == DeclKind::ClassTemplatePartialSpecialization
                || (kind == DeclKind::CXXRecord
                    && (ast.described_template(x).is_some() || ast.partial_spec_info(x).is_some()))
            {
                return true;
            }
            if kind == DeclKind::TranslationUnit || kind == DeclKind::Namespace {
                return false;
            }
            c = ast.decls.lexical_ctx(x);
        }
        false
    }

    /// The parent of `member` that is the namespace `ns`, keyed from the
    /// member's point of view (rule 2 of `spec/verify.md` §2.1): with the
    /// rules along the chain of namespaces if the member is owned, with
    /// none if it is a dependency entity, whoever else opens them. The
    /// namespace's own key when the views agree (the namespace's own unit
    /// is a member of the namespace around it like any other entity), so
    /// that every member of one ownership has one parent key. `view` is
    /// the view already chosen when `member` is itself a namespace keyed
    /// in one. Under no rule, the namespace's key.
    fn namespace_parent(
        &mut self,
        ns: DeclId,
        member: DeclId,
        view: Option<bool>,
    ) -> Result<(Parent, Option<Cause>), Option<Cause>> {
        if self.rules.is_empty() {
            return self.unit_parent(ns);
        }
        let b = match view {
            Some(b) => b,
            None => self.owned(member),
        };
        let res = if b == self.owned(ns) { self.key(ns) } else { self.key_in(ns, Some(b)) };
        match res {
            KeyRes::Key { id, cause, .. } => Ok((Parent::Unit(id), cause)),
            KeyRes::None(c) => Err(Some(c.unwrap_or(BAD_PARENT))),
        }
    }

    fn unit_parent(&mut self, x: DeclId) -> Result<(Parent, Option<Cause>), Option<Cause>> {
        match self.key(x) {
            KeyRes::Key { id, cause, .. } => Ok((Parent::Unit(id), cause)),
            KeyRes::None(c) => Err(Some(c.unwrap_or(BAD_PARENT))),
        }
    }

    /// The first declaration is lexically inside `extern "C"`, the entity
    /// is no class member, and its name has external linkage
    /// (digest-kinds §5.2, [dcl.link]/1). A block-scope declaration looks
    /// through the functions around it.
    fn in_c_linkage(&self, e: DeclId) -> bool {
        let ast = self.ast;
        if ast.decls.get(e).ctx().is_some_and(|c| ast.decls.record(c).is_some()) {
            return false;
        }
        if !self.external_for_c(e) {
            return false;
        }
        match self.linkage_spec(self.representative(e), false) {
            Some((_, Some(LinkageLang::C))) => true,
            Some((_, Some(LinkageLang::Cxx) | None)) | None => false,
        }
    }

    /// The innermost `LinkageSpec` that lexically encloses `d`, with its
    /// language (`ScopeData` `aux-lang`; `None` for a byte that is no
    /// `LinkageLang`, which the payload makes `Unaccounted`). With `all` the search goes
    /// through every enclosing class and function (digest-kinds §5.6, the
    /// language a function's or variable's stream holds); without it, as
    /// the key's parent needs it (§5.2): through namespaces and
    /// transparent scopes only, and through the functions around a
    /// block-scope declaration.
    pub(crate) fn linkage_spec(
        &self,
        d: DeclId,
        all: bool,
    ) -> Option<(DeclId, Option<LinkageLang>)> {
        let ast = self.ast;
        let local_extern = !all && self.is_local_extern(d);
        let mut c = ast.decls.lexical_ctx(d);
        for _ in 0..CTX_LIMIT {
            let x = c?;
            let n = ast.decls.get(x);
            if local_extern && info(n.kind()).local_scope {
                c = ast.decls.lexical_ctx(x);
                continue;
            }
            let through = match info(n.kind()).role {
                Role::Root => return None,
                Role::Transparent | Role::Entity(KeyKind::Namespace) => true,
                Role::Entity(_)
                | Role::Nothing
                | Role::Content(_)
                | Role::Local
                | Role::InParent
                | Role::Unsupported(_) => all,
            };
            if !through {
                return None;
            }
            if n.kind() == DeclKind::LinkageSpec {
                let lang = ast
                    .decls
                    .scope(x)
                    .and_then(slip_kiln::kiln::ast::decl::ScopeData::linkage_lang);
                return Some((x, lang));
            }
            c = ast.decls.lexical_ctx(x);
        }
        None
    }

    /// Whether the name of `e`, a function or variable, can have external
    /// linkage, as far as [dcl.link]/1 needs it: not `static`, not in an
    /// unnamed namespace, and not a namespace-scope `const` variable
    /// declared neither `extern` nor in a one-declaration linkage
    /// specification ([basic.link]/3). Kiln has no public linkage query
    /// (its linkage bits in `DeclMisc` are a cache).
    fn external_for_c(&self, e: DeclId) -> bool {
        let ast = self.ast;
        for d in self.redecls(e) {
            let n = ast.decls.get(d);
            let kind = n.kind();
            if kind == DeclKind::Function
                && ast.decls.function(d).is_some_and(|f| f.spec.storage() == FnStorage::Static)
            {
                return false;
            }
            if kind == DeclKind::Var
                && ast.decls.var(d).is_some_and(|v| v.spec.storage() == VarStorage::Static)
            {
                return false;
            }
        }
        let first = self.representative(e);
        let n = ast.decls.get(first);
        if n.kind() == DeclKind::Var && !self.is_local_extern(first) {
            let explicit_extern = self.redecls(e).iter().any(|&d| {
                ast.decls.var(d).is_some_and(|v| v.spec.storage() == VarStorage::Extern)
                    || ast.decls.is_single_line_linkage(d)
            });
            let constant = n.ty().is_some_and(|q| ast.types.canonical(q).quals().is_const());
            if constant && !explicit_extern {
                return false;
            }
        }
        let mut c = n.ctx();
        for _ in 0..CTX_LIMIT {
            let Some(x) = c else { return true };
            let xn = ast.decls.get(x);
            if xn.kind() == DeclKind::Namespace
                && (xn.name().is_empty()
                    || ast
                        .decls
                        .namespace(x)
                        .is_some_and(|ns| ns.flags & NamespaceData::ANONYMOUS != 0))
            {
                return false;
            }
            c = xn.ctx();
        }
        true
    }

    /// A named entity's name, or for one without a name the forms of
    /// digest §3.3 (`Err` when none applies).
    fn entity_name(&mut self, e: DeclId) -> Result<(Name, Option<Cause>), Option<Cause>> {
        // A local type is `local(n)`, whatever its name (digest §4.3).
        if crate::local::is_local_type(self, e) {
            return crate::local::type_number(self, e)
                .map(|n| (Name::Local(n), None))
                .map_err(Some);
        }
        if let Some(named) = self.plain_name(e).map_err(Some)? {
            return Ok(named);
        }
        let ast = self.ast;
        let kind = ast.decls.get(e).kind();
        match info(kind).role {
            Role::Entity(KeyKind::Namespace) => Ok((Name::Anonymous, None)),
            Role::Entity(KeyKind::Class | KeyKind::Enumeration) => self.unnamed_tag(e),
            Role::Entity(KeyKind::Field) => self.unnamed_field(e),
            Role::Entity(KeyKind::Variable) => self.unnamed_variable(e),
            Role::Entity(_)
            | Role::Nothing
            | Role::Transparent
            | Role::Root
            | Role::Content(_)
            | Role::Local
            | Role::InParent
            | Role::Unsupported(_) => Err(Some(BAD_NAME)),
        }
    }

    /// The `Name` a declaration's own name is (digest-kinds §3.2); `None`
    /// for a declaration without one.
    pub(crate) fn plain_name(&mut self, d: DeclId) -> Result<Option<(Name, Option<Cause>)>, Cause> {
        let ast = self.ast;
        let name = ast.decls.get(d).name();
        Ok(Some(match ast.names.view(name) {
            DeclNameView::Empty => return Ok(None),
            DeclNameView::Identifier(sym) => {
                let text = ast.symbols().as_str(sym);
                if text.is_empty() {
                    return Ok(None);
                }
                (Name::Identifier(text.as_bytes().to_vec()), None)
            }
            DeclNameView::Operator(op) => {
                (Name::Operator(u64::from(overloaded_operator::row(&op).tag)), None)
            }
            DeclNameView::Constructor(_) => (Name::Constructor, None),
            DeclNameView::Destructor(_) => (Name::Destructor, None),
            DeclNameView::Conversion(q) => {
                let t = self.type_digest(q, false);
                (Name::Conversion(t.digest), t.cause)
            }
            DeclNameView::LiteralOperator(sym) => {
                (Name::LiteralOperator(ast.symbols().as_str(sym).as_bytes().to_vec()), None)
            }
            DeclNameView::DeductionGuide(_) => (Name::DeductionGuide, None),
        }))
    }

    /// An unnamed class or enumeration (digest §3.3): the typedef name
    /// for linkage, else the first declarator, else the first enumerator
    /// or member, else the ordinal among the scope's unnamed types.
    fn unnamed_tag(&mut self, e: DeclId) -> Result<(Name, Option<Cause>), Option<Cause>> {
        if let Some(by) = self.unnamed_tag_by(e) {
            let (name, cause) = self.plain_name(by).map_err(Some)?.ok_or(Some(BAD_NAME))?;
            return Ok((Name::UnnamedBy(Box::new(name)), cause));
        }
        let ordinal = self.ordinal_in_scope(e).ok_or(Some(BAD_NAME))?;
        Ok((Name::UnnamedOrdinal(ordinal), None))
    }

    /// The declaration whose name names unnamed tag `e`, by the first
    /// three rules of digest §3.3; a name-independent declarator or
    /// member is skipped.
    pub(crate) fn unnamed_tag_by(&self, e: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        if let Some(td) = ast.decls.typedef_for_anon(e) {
            return Some(td);
        }
        let named = |d: DeclId| !ast.decls.get(d).name().is_empty();
        // A name-independent declarator (`struct { int a; } _;`, P2169)
        // or member names nothing: the next rule applies (WP-039).
        if let Some(next) = ast.decls.get(e).next()
            && named(next)
            && !ast.decls.get(next).misc().name_independent()
            && let Some(q) = ast.decls.get(next).ty()
            && self.declarator_of(q) == Some(e)
        {
            return Some(next);
        }
        let holder = ast.definition(e).unwrap_or(e);
        ast.members(holder).find(|&m| {
            let kind = ast.decls.get(m).kind();
            let member = match info(kind).role {
                Role::Entity(_) | Role::Content(_) => true,
                Role::Nothing
                | Role::Transparent
                | Role::Root
                | Role::Local
                | Role::InParent
                | Role::Unsupported(_) => false,
            };
            member && named(m) && !ast.decls.get(m).misc().name_independent()
        })
    }

    /// The tag a declarator's type declares an object of, through
    /// pointers, references, arrays and member pointers.
    fn declarator_of(&self, q: QualType) -> Option<DeclId> {
        let types = &self.ast.types;
        let mut q = types.canonical(q);
        for _ in 0..CTX_LIMIT {
            match types.view(q.ty()) {
                TypeView::Record(d) | TypeView::Enum(d) => return Some(self.ast.first(d)),
                TypeView::Pointer(x)
                | TypeView::LValueRef(x)
                | TypeView::RValueRef(x)
                | TypeView::IncompleteArray(x)
                | TypeView::MemberPointer { pointee: x, class: _ } => q = types.canonical(x),
                TypeView::ConstantArray(a) => q = types.canonical(a.elem),
                TypeView::ExtQual { base, word: _ } => q = types.canonical(base),
                TypeView::Invalid
                | TypeView::Builtin(_)
                | TypeView::BitInt { .. }
                | TypeView::DependentBitInt { .. }
                | TypeView::VariableArray { .. }
                | TypeView::DependentSizedArray { .. }
                | TypeView::Function(_)
                | TypeView::UnresolvedUsing(_)
                | TypeView::Paren(_)
                | TypeView::Complex(_)
                | TypeView::Atomic(_)
                | TypeView::Vector { .. }
                | TypeView::DependentVector { .. }
                | TypeView::DependentAddrSpace { .. }
                | TypeView::Typedef { .. }
                | TypeView::Using { .. }
                | TypeView::Elaborated { .. }
                | TypeView::QualifiedArray { .. }
                | TypeView::TemplateTypeParm(_)
                | TypeView::CanonTemplateTypeParm { .. }
                | TypeView::SubstTemplateTypeParm { .. }
                | TypeView::SubstTemplateTypeParmPack { .. }
                | TypeView::TemplateSpecialization { .. }
                | TypeView::InjectedClassName { .. }
                | TypeView::DependentName { .. }
                | TypeView::DependentTemplateSpecialization { .. }
                | TypeView::PackExpansion { .. }
                | TypeView::PackIndexing { .. }
                | TypeView::Decltype { .. }
                | TypeView::DependentDecltype(_)
                | TypeView::TypeOf { .. }
                | TypeView::UnaryTransform { .. }
                | TypeView::Auto { .. }
                | TypeView::DeducedTemplateSpecialization { .. }
                | TypeView::Attributed { .. }
                | TypeView::Splice { .. } => return None,
            }
        }
        None
    }

    /// An unnamed field: named by its anonymous class, else by its
    /// ordinal among the class's unnamed fields (digest-kinds §5.2).
    fn unnamed_field(&mut self, e: DeclId) -> Result<(Name, Option<Cause>), Option<Cause>> {
        let ast = self.ast;
        if let Some(q) = ast.decls.get(e).ty()
            && let TypeView::Record(r) = ast.types.view(ast.types.canonical(q).ty())
        {
            let r = ast.first(r);
            if ast.decls.get(r).name().is_empty() {
                let (name, cause) = self.entity_name(r)?;
                return Ok((Name::UnnamedBy(Box::new(name)), cause));
            }
        }
        let ordinal = self.ordinal_in_scope(e).ok_or(Some(BAD_NAME))?;
        Ok((Name::UnnamedOrdinal(ordinal), None))
    }

    /// An unnamed variable: the object of an anonymous union at namespace
    /// scope (`static union { int a; };`), named by its union as an
    /// unnamed field is (digest-kinds §5.2).
    fn unnamed_variable(&mut self, e: DeclId) -> Result<(Name, Option<Cause>), Option<Cause>> {
        let ast = self.ast;
        if let Some(q) = ast.decls.get(e).ty()
            && let TypeView::Record(r) = ast.types.view(ast.types.canonical(q).ty())
        {
            let r = ast.first(r);
            if ast.decls.get(r).name().is_empty() {
                let (name, cause) = self.entity_name(r)?;
                return Ok((Name::UnnamedBy(Box::new(name)), cause));
            }
        }
        Err(Some(BAD_NAME))
    }

    /// The ordinal of `e` among the unnamed types (for a field, the
    /// unnamed fields) of its scope that no other rule names, from 0, in
    /// declaration order over every declaration of the scope.
    fn ordinal_in_scope(&mut self, e: DeclId) -> Option<u64> {
        let ast = self.ast;
        let scope = ast.decls.get(e).ctx()?;
        let scope = self.entity(scope);
        if self.ordinal_scopes.insert(scope) {
            let mut holders: Vec<DeclId> =
                if ast.decls.redecl(scope).is_some() { self.redecls(scope) } else { vec![scope] };
            holders.reverse();
            let (mut types, mut nfields) = (0u64, 0u64);
            let mut stack: Vec<DeclId> = Vec::new();
            for h in holders {
                stack.extend(ast.members(h));
                stack.reverse();
                while let Some(m) = stack.pop() {
                    let mn = ast.decls.get(m);
                    let role = info(mn.kind()).role;
                    if role == Role::Transparent {
                        let mut inner: Vec<DeclId> = ast.members(m).collect();
                        inner.reverse();
                        stack.extend(inner);
                        continue;
                    }
                    if !mn.name().is_empty() {
                        continue;
                    }
                    match role {
                        Role::Entity(KeyKind::Class | KeyKind::Enumeration)
                            if !self.is_local(m) && self.unnamed_tag_by(m).is_none() =>
                        {
                            self.ordinal_of.insert(m, types);
                            types += 1;
                        }
                        Role::Entity(KeyKind::Field) if mn.kind() == DeclKind::Field => {
                            let anon = mn.ty().is_some_and(|q| {
                                if let TypeView::Record(r) =
                                    ast.types.view(ast.types.canonical(q).ty())
                                {
                                    ast.decls.get(r).name().is_empty()
                                } else {
                                    false
                                }
                            });
                            if !anon {
                                self.ordinal_of.insert(m, nfields);
                                nfields += 1;
                            }
                        }
                        Role::Entity(_)
                        | Role::Nothing
                        | Role::Transparent
                        | Role::Root
                        | Role::Content(_)
                        | Role::Local
                        | Role::InParent
                        | Role::Unsupported(_) => {}
                    }
                }
            }
        }
        self.ordinal_of.get(&e).copied()
    }

    /// The discriminator of function template `e` (digest-kinds §3.3): its
    /// template head, then its pattern's function discriminator and return
    /// type.
    fn function_template_disc(&mut self, s: &mut Stream, e: DeclId) {
        let ast = self.ast;
        let info = ast.template_info(e).copied();
        match info {
            Some(i) => {
                crate::templ::template_head(self, s, i.params);
                match i.pattern {
                    Some(p) => {
                        let befriended = self.friend_class(e).is_some();
                        self.function_disc(s, p, befriended);
                        let ret = ast.decls.get(p).ty().and_then(|q| {
                            ast.types.proto(q).map(slip_kiln::kiln::ast::types::FuncProto::ret)
                        });
                        self.write_type_opt(s, ret);
                    }
                    None => {
                        s.indeterminate(Cause::Unaccounted("key/function-template"));
                    }
                }
            }
            None => {
                s.indeterminate(Cause::Unaccounted("key/function-template"));
            }
        }
    }

    /// The discriminator of entity `e` of key kind `kind` (digest-kinds
    /// §3.3), with the cause it carries; `None` when the kind has none.
    fn disc(
        &mut self,
        e: DeclId,
        kind: KeyKind,
        c_linkage: bool,
    ) -> Option<(Digest, Option<Cause>)> {
        let ast = self.ast;
        let mut s = Stream::new();
        match kind {
            KeyKind::Function => {
                if c_linkage {
                    return None;
                }
                let befriended = self.friend_class(e).is_some();
                self.function_disc(&mut s, e, befriended);
            }
            KeyKind::DeductionGuide => {
                let template = match ast.names.view(ast.decls.get(e).name()) {
                    DeclNameView::DeductionGuide(t) => Some(t),
                    DeclNameView::Empty
                    | DeclNameView::Identifier(_)
                    | DeclNameView::Operator(_)
                    | DeclNameView::Constructor(_)
                    | DeclNameView::Destructor(_)
                    | DeclNameView::Conversion(_)
                    | DeclNameView::LiteralOperator(_) => None,
                };
                self.write_ref_opt(&mut s, template);
                self.function_disc(&mut s, e, false);
            }
            KeyKind::FunctionTemplate => self.function_template_disc(&mut s, e),
            KeyKind::ClassTemplate
            | KeyKind::VariableTemplate
            | KeyKind::AliasTemplate
            | KeyKind::Concept => match ast.template_info(e).copied() {
                Some(i) => crate::templ::template_head(self, &mut s, i.params),
                None if ast.decls.get(e).kind() == DeclKind::BuiltinTemplate => return None,
                None => {
                    s.indeterminate(Cause::Unaccounted("key/template"));
                }
            },
            KeyKind::UsingDeclaration => {
                let q = ast.using_qualifier(e);
                s.opt(q.is_some());
                if let Some(q) = q {
                    self.write_nns(&mut s, q);
                }
            }
            KeyKind::UsingShadow => {
                let target = ast.using_target(e);
                self.write_ref_opt(&mut s, target);
            }
            KeyKind::Field if ast.decls.get(e).misc().name_independent() => {
                match self.name_independent_ordinal(e) {
                    Some(n) => {
                        s.uv(n);
                    }
                    None => {
                        s.uv(u64::MAX).indeterminate(Cause::Unaccounted("key/name-independent"));
                    }
                }
            }
            KeyKind::Namespace
            | KeyKind::Class
            | KeyKind::Enumeration
            | KeyKind::Enumerator
            | KeyKind::Variable
            | KeyKind::Field
            | KeyKind::Alias
            | KeyKind::Specialisation
            | KeyKind::PartialSpecialisation
            | KeyKind::UsingEnum
            | KeyKind::NamespaceAlias
            | KeyKind::Binding
            | KeyKind::Decomposition
            | KeyKind::UsingDirective
            | KeyKind::StaticAssert
            | KeyKind::ExplicitInstantiation
            | KeyKind::Friend
            | KeyKind::FileScopeAsm
            | KeyKind::ConstevalBlock
            | KeyKind::Resolved
            | KeyKind::Order
            | KeyKind::DefaultArgument
            | KeyKind::TemplateParamObject
            | KeyKind::UnresolvedIfExists => return None,
        }
        let h = s.finish(DISC_TAG);
        let h = self.hashed(h);
        Some((h.digest, h.cause))
    }

    /// The ordinal of name-independent member `e` (P2169: a member named
    /// `_` in C++26, [basic.scope.scope]/5) among the name-independent
    /// members of its kind and name in its class, in member order: two
    /// of them in one class are two entities of one name, so the ordinal
    /// is the discriminator that tells them apart (`spec/digest-kinds.md`
    /// §3.3 `field`). The members are those of the definition of the
    /// record `e` is a member of (the pattern's for a member of a class
    /// template, whose entity is the template), else of the definition of
    /// that record's entity. `None` if `e` is in neither.
    pub(crate) fn name_independent_ordinal(&self, e: DeclId) -> Option<u64> {
        let ast = self.ast;
        let n = ast.decls.get(e);
        let ctx = n.ctx()?;
        let entity = self.entity(ctx);
        let holders =
            [ast.definition(ctx).unwrap_or(ctx), ast.definition(entity).unwrap_or(entity)];
        holders.into_iter().find_map(|holder| {
            let mut ordinal = 0u64;
            for m in ast.members(holder) {
                if m == e {
                    return Some(ordinal);
                }
                let mn = ast.decls.get(m);
                if mn.kind() == n.kind() && mn.name() == n.name() && mn.misc().name_independent() {
                    ordinal += 1;
                }
            }
            None
        })
    }

    /// The function discriminator of digest-kinds §3.3, written into `s`:
    /// parameter types, variadic, the cv-qualifiers of `this` as a flag
    /// word, the ref-qualifier, the explicit-object flag and the trailing
    /// requires-clause.
    /// `befriended`: the function is a friend whose parent is the class
    /// that befriends it (`friend_class`), which appends `uv` 1 after the
    /// requires-clause (§3.3 `function`), so that it never has a member's
    /// key.
    pub(crate) fn function_disc(&mut self, s: &mut Stream, f: DeclId, befriended: bool) {
        let ast = self.ast;
        let proto =
            ast.decls.get(f).ty().and_then(|q| match ast.types.view(ast.types.canonical(q).ty()) {
                TypeView::Function(p) => Some(p),
                TypeView::Invalid
                | TypeView::Builtin(_)
                | TypeView::BitInt { .. }
                | TypeView::DependentBitInt { .. }
                | TypeView::Pointer(_)
                | TypeView::LValueRef(_)
                | TypeView::RValueRef(_)
                | TypeView::MemberPointer { .. }
                | TypeView::ConstantArray(_)
                | TypeView::IncompleteArray(_)
                | TypeView::VariableArray { .. }
                | TypeView::DependentSizedArray { .. }
                | TypeView::Record(_)
                | TypeView::Enum(_)
                | TypeView::UnresolvedUsing(_)
                | TypeView::Paren(_)
                | TypeView::Complex(_)
                | TypeView::Atomic(_)
                | TypeView::Vector { .. }
                | TypeView::DependentVector { .. }
                | TypeView::ExtQual { .. }
                | TypeView::DependentAddrSpace { .. }
                | TypeView::Typedef { .. }
                | TypeView::Using { .. }
                | TypeView::Elaborated { .. }
                | TypeView::QualifiedArray { .. }
                | TypeView::TemplateTypeParm(_)
                | TypeView::CanonTemplateTypeParm { .. }
                | TypeView::SubstTemplateTypeParm { .. }
                | TypeView::SubstTemplateTypeParmPack { .. }
                | TypeView::TemplateSpecialization { .. }
                | TypeView::InjectedClassName { .. }
                | TypeView::DependentName { .. }
                | TypeView::DependentTemplateSpecialization { .. }
                | TypeView::PackExpansion { .. }
                | TypeView::PackIndexing { .. }
                | TypeView::Decltype { .. }
                | TypeView::DependentDecltype(_)
                | TypeView::TypeOf { .. }
                | TypeView::UnaryTransform { .. }
                | TypeView::Auto { .. }
                | TypeView::DeducedTemplateSpecialization { .. }
                | TypeView::Attributed { .. }
                | TypeView::Splice { .. } => None,
            });
        let Some(p) = proto else {
            s.indeterminate(Cause::Unaccounted("key/function-type"));
            return;
        };
        let params = ast.types.params(&p).to_vec();
        self.write_types(s, &params);
        let fi = p.info();
        s.bool(fi.variadic());
        let q = fi.this_quals();
        s.flags(FlagWord::new().bit(q.is_const()).bit(q.is_volatile()).bit(q.is_restrict()));
        s.tag(ref_qual::row(&fi.ref_qual()));
        let explicit = ast.decls.function(f).is_some_and(|d| d.spec.has_explicit_object());
        s.bool(explicit);
        let requires = ast.templates.trailing_requires.get(f).copied();
        s.opt(requires.is_some());
        if let Some(r) = requires {
            crate::expr_dep::profile_expr(self, s, r);
        }
        if befriended {
            s.uv(1);
        }
    }
}

/// Collisions that the rules make among keys no unit has, or between such
/// a key and a unit's (`spec/digest.md` §3.4 "Two different entities",
/// WP-103): after the walk, the entities whose mapped keys the walk
/// computed are grouped by mapped `KeyId`; entities whose keys under no
/// rule are equal are one entity (several declarations Kiln does not
/// join, such as a friend that each class specialisation declares). A
/// group of two or more entities, not all namespaces (which merge), at
/// least one of which has no unit under that key, is a collision:
/// `units::mark_collisions` sees units only, and two references that the
/// rules make into one would otherwise be equal in silence (a
/// specialisation only named, a function a block-scope `extern`
/// declaration in a templated function declares, digest §2.2). Every
/// unit with the key is marked `Collision`; if none has it, the order
/// unit is, so that the collision is a finding (§10). Returns how many
/// keys collide that `mark_collisions` did not already count. With no
/// rule the mapped and the unmapped keys are equal and nothing is marked.
pub(crate) fn mark_mapped_collisions(
    w: &Walk<'_>,
    input: &crate::Input<'_>,
    written: &mut [crate::units::Written],
) -> u64 {
    if w.rules.is_empty() {
        return 0;
    }
    let mut keyed: Vec<(Digest, DeclId, KeyKind)> = w
        .keys
        .iter()
        .filter_map(|(&e, r)| match *r {
            KeyRes::Key { id, kind, .. } => Some((id, e, kind)),
            KeyRes::None(_) => None,
        })
        .collect();
    keyed.sort_by_key(|&(id, e, _)| (id, e.index()));
    let empty = slip_model::keys::RuleTable::empty();
    let mut plain = Walk::new(input, &empty);
    let mut collisions = 0;
    let mut i = 0;
    while i < keyed.len() {
        let id = keyed[i].0;
        let mut j = i + 1;
        while j < keyed.len() && keyed[j].0 == id {
            j += 1;
        }
        let group = &keyed[i..j];
        i = j;
        if group.len() < 2 || group.iter().all(|g| g.2 == KeyKind::Namespace) {
            continue;
        }
        // The entities of the group, one per key under no rule (`None`: a
        // key the walk without rules cannot form, an entity of its own),
        // and whether one of each has a unit under `id`.
        let mut classes: Vec<(Option<Digest>, bool)> = Vec::new();
        for &(_, e, _) in group {
            let unmapped = match plain.key(e) {
                KeyRes::Key { id: u, .. } => Some(u),
                KeyRes::None(_) => None,
            };
            let has_unit = written.iter().any(|u| u.key == id && u.entity == Some(e));
            match classes.iter_mut().find(|c| c.0.is_some() && c.0 == unmapped) {
                Some(c) => c.1 |= has_unit,
                None => classes.push((unmapped, has_unit)),
            }
        }
        if classes.len() < 2 || classes.iter().all(|c| c.1) {
            continue;
        }
        let already = written.iter().any(|u| u.key == id && u.cause == Some(Cause::Collision));
        let mut marked = false;
        for u in written.iter_mut().filter(|u| u.key == id) {
            u.cause = Some(crate::units::merge(u.cause, Cause::Collision));
            marked = true;
        }
        if !marked && let Some(u) = written.iter_mut().find(|u| u.kind == KeyKind::Order) {
            u.cause = Some(crate::units::merge(u.cause, Cause::Collision));
        }
        if !already {
            collisions += 1;
        }
    }
    collisions
}

/// `merge` of two optional causes, as `Stream::indeterminate` keeps them.
fn merge(a: Option<Cause>, b: Option<Cause>) -> Option<Cause> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(x), Some(y)) => Some(crate::units::merge(Some(x), y)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use slip_kiln::kiln::ast::names::{DeclNameView, OverloadedOperator};
    use slip_model::keys::{KeyKind, Name, RuleTable};

    use crate::Input;
    use crate::tables::overloaded_operator;
    use crate::walk::Walk;

    /// Every overloadable operator, as a member of `S` (the allocation
    /// functions static), one signature per group.
    const OPERATORS: &str = "using Z = decltype(sizeof 0);\nstruct S {\n\
        static void *operator new(Z); static void *operator new[](Z);\n\
        static void operator delete(void *); static void operator delete[](void *);\n\
        S operator~(); S operator!(); S operator->(); S operator++(); S operator--();\n\
        S operator co_await();\n\
        S operator+(int); S operator-(int); S operator*(int); S operator/(int);\n\
        S operator%(int); S operator^(int); S operator&(int); S operator|(int);\n\
        S operator=(int); S operator<(int); S operator>(int); S operator+=(int);\n\
        S operator-=(int); S operator*=(int); S operator/=(int); S operator%=(int);\n\
        S operator^=(int); S operator&=(int); S operator|=(int); S operator<<(int);\n\
        S operator>>(int); S operator<<=(int); S operator>>=(int); S operator==(int);\n\
        S operator!=(int); S operator<=(int); S operator>=(int); S operator<=>(int);\n\
        S operator&&(int); S operator||(int); S operator,(int); S operator->*(int);\n\
        S operator()(int); S operator[](int);\n};\n";

    /// digest-kinds §3.2, §10.2 (WP-039): the key of `operator<op>` is
    /// a function key named `operator` with the tag of `op`'s
    /// `OverloadedOperator` row, for every row; no two operators share
    /// a key name.
    #[test]
    fn the_key_of_an_operator_holds_its_row() {
        let files = [("main.cpp", OPERATORS)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let rules = RuleTable::empty();
        let seen = slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            let (mut seen, mut names) = (BTreeSet::new(), BTreeSet::new());
            for d in p.ast.decls.ids() {
                let DeclNameView::Operator(op) = p.ast.names.view(p.ast.decls.get(d).name()) else {
                    continue;
                };
                let want = Name::Operator(u64::from(overloaded_operator::row(&op).tag));
                let (key, cause) = w.compute_key(d).expect("an operator has a key");
                assert_eq!(cause, None, "{op:?}");
                assert_eq!(key.kind, KeyKind::Function, "{op:?}");
                assert_eq!(key.name, want, "{op:?}");
                assert!(seen.insert(op as u8), "{op:?} declared once");
                assert!(names.insert(key.name), "{op:?}: its key name is another operator's");
            }
            seen
        })
        .expect("the snippet parses");
        let every: BTreeSet<u8> =
            (1..=u8::MAX).filter_map(OverloadedOperator::from_u8).map(|op| op as u8).collect();
        assert_eq!(seen, every, "every row is declared");
    }
}
