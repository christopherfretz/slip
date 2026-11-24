//! `spec/edits.md` §9: the native rename of functions and variables
//! (`docs/wp/WP-071.md`, `spec/landed/edits/WP-071.md`).
//!
//! One run, in this order:
//!
//! 1. [`target::seeds`]: one pass over `written::decl_names` and one over
//!    `written::written_refs`. An entry is looked at only when the AST
//!    names it (the declaration, or the origin of the referent) like the
//!    old name; its entity is then matched against the target over keys
//!    (§9.1), each entity once.
//! 2. The hooks, each in a file of its own, which the packages that
//!    follow replace: [`families::collect`], [`followers::collect`],
//!    [`spec_members::collect`].
//! 3. The flags the core sets from what the unit shows
//!    ([`subjects::flag_unreachable`], [`subjects::flag_name_taken`]).
//! 4. The claims: every declaration named like the old name
//!    ([`decls::claim`]), every written reference whose referent is
//!    ([`refs::claim`]), and every dependent name ([`dependent::claim`]).
//! 5. The section: derivations joined into claims (§4.3), files indexed,
//!    `Edits::normalise`.

mod claims;
mod decls;
mod dependent;
mod families;
mod followers;
mod refs;
mod spec_members;
mod subjects;
mod target;

use std::collections::HashSet;

use slip_digest::query::Keys;
use slip_kiln::Unit;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::kiln::source::FileId;
use slip_kiln::sites::Sites;
use slip_kiln::written::{self, DeclName};
use slip_model::edits::{Changed, Edits, SCHEME};
use slip_model::transform::{Rename, TransformKind};
use slip_model::{FileRef, Transform};

pub(crate) use claims::{Claims, Written};
pub(crate) use subjects::Subjects;

/// What one run reads and writes. The fields up to `changed` are those
/// `docs/wp/WP-071.md` fixes for the packages that follow; `named` and
/// `settled` are this package's additions (documented in the landed
/// file).
pub(crate) struct Ctx<'a, 'k, 'r> {
    pub unit: &'a Unit<'r>,
    pub keys: &'a mut Keys<'k>,
    pub sites: Sites<'a, 'r>,
    pub request: &'a Rename,
    pub old: &'a [u8],
    pub new: &'a [u8],
    pub subjects: Subjects,
    pub claims: Claims,
    pub changed: Vec<Changed>,
    /// The entries of `written::decl_names` the AST names like the old
    /// name and like the new name, in its order (one pass, `target`).
    pub named: Named,
    /// Declarations a hook has dealt with: [`decls::claim`] claims
    /// nothing for them.
    pub settled: HashSet<DeclId>,
}

/// The declaration names of the unit that matter to the rename.
#[derive(Debug, Default)]
pub(crate) struct Named {
    /// Named like the old name: the declarations the rename claims at.
    pub old: Vec<DeclName>,
    /// Named like the new name: what `name_taken` is decided from.
    pub new: Vec<DeclName>,
}

/// `crate::run_counted`: the section, and the number of written
/// references looked at.
pub(crate) fn run(
    transform: &Transform,
    unit: &Unit<'_>,
    keys: &mut Keys<'_>,
    files: &dyn Fn(FileId) -> Option<FileRef>,
) -> Result<(Edits, u64), String> {
    if transform.kind != TransformKind::Rename {
        return Err(format!("slip_refactor runs a rename; the request is a {:?}", transform.kind));
    }
    let Some(request) = &transform.rename else {
        return Err("the request is a rename without its rename record".to_owned());
    };
    // The §3 rules (an identifier, not a keyword, not the old name) are
    // the request's validity: a request that breaks them never reaches
    // `Keys::rename_rule`.
    transform.validate().map_err(|e| format!("the request is not valid: {e}"))?;
    let id = slip_model::formats::content_id(&transform.to_bytes())
        .map_err(|e| format!("the request has no content id: {e}"))?;
    if !written::recorded(unit) {
        return Err("the unit was parsed without ParseOptions::written".to_owned());
    }
    // `name_taken` asks the preprocessing record for a macro named like
    // the new name (§9.5): a guard must not depend on the caller
    // remembering an option.
    if unit.pp_record().is_none() {
        return Err("the unit was parsed without the preprocessing record \
                    (ParseOptions::pp_record)"
            .to_owned());
    }
    let main = unit.main_file().ok_or("the unit has no main file")?;
    let old = request.target.last().ok_or("the request's target is empty")?;

    let mut ctx = Ctx {
        unit,
        keys,
        sites: Sites::new(unit, &transform.ownership),
        request,
        old,
        new: &request.new_name,
        subjects: Subjects::default(),
        claims: Claims::new(old, &request.new_name),
        changed: Vec::new(),
        named: Named::default(),
        settled: HashSet::new(),
    };

    let (hits, looked) = target::seeds(&mut ctx);
    families::collect(&mut ctx);
    followers::collect(&mut ctx);
    spec_members::collect(&mut ctx);
    subjects::flag_unreachable(&mut ctx);
    subjects::flag_name_taken(&mut ctx);

    decls::claim(&mut ctx);
    refs::claim(&mut ctx, &hits);
    let dependent = written::dependent_names(unit);
    let bound = dependent::bound(&ctx, &dependent);
    for name in &dependent {
        dependent::claim(&mut ctx, name, &bound);
    }

    let Ctx { subjects, claims, changed, .. } = ctx;
    let (files, claims) = claims.finish(files, main)?;
    let (subjects, links) = subjects.finish(unit.ast());
    let mut edits = Edits {
        scheme: SCHEME,
        transform: id,
        failure: None,
        files,
        subjects,
        links,
        claims,
        groups: Vec::new(),
        reports: Vec::new(),
        changed,
        renamed: Vec::new(),
    };
    edits.normalise();
    Ok((edits, looked))
}

/// The scope the AST says `d` is declared in: its semantic context, a
/// linkage specification or an `export` block skipped (neither is a
/// scope of names). `None` for the translation unit.
pub(crate) fn enclosing(ast: &Ast, d: DeclId) -> Option<DeclId> {
    let mut c = ast.decls.get(d).ctx()?;
    // A chain of contexts is finite (each is a declaration that encloses
    // the last); the bound only guards a malformed tree.
    for _ in 0..CTX_LIMIT {
        match ast.decls.get(c).kind() {
            DeclKind::LinkageSpec | DeclKind::Export => c = ast.decls.get(c).ctx()?,
            _ => return Some(c),
        }
    }
    None
}

/// `written::origin` of `d`; for a **variable** that has no link of its
/// own (origin takes no step), `written::origin` of its entity.
///
/// The one case this is for: the explicit specialisation of a static
/// data member, `template <> int S<int>::v = 1;`, is a redeclaration of
/// the member of the instantiation that the front end does not link (its
/// entity is linked). `written::origin` stops at it, so its claim named
/// the follower, while a member function's explicit specialisation,
/// which is linked, names the seed. Through the entity it names the seed
/// as well (`spec/edits.md` §9.2).
///
/// Here rename differs from `written::origin`, which the host and the
/// check share ("one function, so that the rename, the host and the check
/// cannot disagree"). Nothing they compare depends on it: the claim of a
/// declaration carries a literal piece, and a reference's spelled key is
/// only changed for a variable whose entity's origin is the seed, so the
/// key the check reads is the seed's, which maps to the same name. The
/// step is limited to variables because for a function it would make
/// the subject depend on instantiation order: a namespace-scope
/// redeclaration `void f(S<int>);` of a friend that the class template
/// defines has, as its entity, the instantiated friend when it follows
/// `S<int> s;` and itself when it precedes it
/// (`s9_3-friend-redeclared-after-instantiation`, `-before-`).
pub(crate) fn origin_of(ctx: &mut Ctx<'_, '_, '_>, d: DeclId) -> DeclId {
    let o = written::origin(ctx.unit, d);
    if o != d || ctx.unit.ast().decls.get(d).kind() != DeclKind::Var {
        return o;
    }
    written::origin(ctx.unit, ctx.keys.entity(d))
}

/// How far a chain of contexts is followed.
pub(crate) const CTX_LIMIT: u32 = 4096;
