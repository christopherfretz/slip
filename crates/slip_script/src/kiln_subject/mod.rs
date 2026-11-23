//! [`KilnSubject`]: the [`Subject`] over one parsed unit of Kiln
//! (`spec/api.md` §5, §6; `docs/wp/WP-083.md`).
//!
//! WP-083 answers entities, lookup and structure (`entities.rs`,
//! `lookup.rs`); WP-085 references, ranges, places and the written
//! declarations (`references.rs`). Every other method answers "nothing"
//! as the trait documents it, until WP-086 (calls) and WP-095 (keys): no
//! expression is live, so the host never asks the methods that take one.
//!
//! Entities are read from Kiln's declaration arena, references from its
//! written-reference and declaration-extent tables (`slip_kiln::written`),
//! which the unit must have recorded. What needs the unit is read when
//! the subject is made; what is costly otherwise is built on first use
//! and kept for the life of the subject: the template parameters' owners
//! (`entities.rs`), one name index per scope that `lookup` reaches
//! (`lookup.rs`), the references by origin and by extent
//! (`references.rs`), each file's bytes.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::kiln::source::{FileId, Loc, SourceManager};
use slip_model::FileRef;
use slip_model::keys::Rule;

use crate::subject::{
    Declaration, Entity, Expression, File, PlaceOf, Range, Reference, Subject, Value,
};
use crate::wire::{self, Errc};

mod entities;
mod lookup;
mod references;

/// Unit order (§1): `slip_kiln::ranges`' comparator (WP-084), which
/// replaced WP-083's own, except that an invalid location (a declaration
/// Kiln made, with no place) sorts after every valid one, as WP-083's did
/// (its mutant M17); `ranges` puts it first.
pub fn unit_order(sm: &SourceManager, a: Loc, b: Loc) -> std::cmp::Ordering {
    match (a.is_valid(), b.is_valid()) {
        (true, true) => slip_kiln::ranges::unit_order(sm, a, b),
        (va, vb) => vb.cmp(&va),
    }
}

/// A file of the subject: its path and its bytes.
type SubjectFile<'u> = (Vec<u8>, Cow<'u, [u8]>);

/// The subject over one unit of Kiln, for entities, lookup and structure.
pub struct KilnSubject<'u> {
    ast: &'u Ast,
    sm: &'u SourceManager,
    /// Per buffer (in `SourceManager::file_ids` order), its subject path
    /// (its real path for a unit of the run); `None` for a buffer that is
    /// no file of the subject. Copied out when the subject is made.
    paths: Vec<Option<Cow<'u, [u8]>>>,
    owned: &'u (dyn Fn(&[u8]) -> bool + Sync),
    /// Per buffer: its path and its bytes, once asked.
    files: Vec<OnceLock<Option<SubjectFile<'u>>>>,
    /// Template parameter → its template (`entities.rs`).
    parm_owner: OnceLock<HashMap<u32, u32>>,
    /// The name index of each scope `lookup` reached (`lookup.rs`). The
    /// lock is held only to read or insert, never while an index is
    /// built: two threads may build one index twice, to the same value.
    scopes: Mutex<HashMap<u32, Arc<lookup::Index>>>,
    /// References, ranges and written declarations (`references.rs`).
    names: references::Names,
}

impl std::fmt::Debug for KilnSubject<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KilnSubject")
            .field("declarations", &self.ast.decls.len())
            .field("scopes_indexed", &self.scopes().len())
            .field("names", &self.names)
            .finish_non_exhaustive()
    }
}

impl<'u> KilnSubject<'u> {
    /// `owned`: the run's ownership, by subject file (its real path).
    /// The subject keeps the unit's AST, source manager and real paths,
    /// and what it read from the unit's tables, not the unit (which holds
    /// the Oven, and is not `Sync`). `Err` when the unit was parsed
    /// without Kiln's written-reference and declaration-extent tables
    /// (`ParseOptions::written`): its references cannot be told.
    pub fn new(
        unit: &slip_kiln::Unit<'u>,
        owned: &'u (dyn Fn(&[u8]) -> bool + Sync),
    ) -> Result<KilnSubject<'u>, String> {
        KilnSubject::from_parts(unit, |f| unit.real_path(f).map(Cow::Borrowed), owned)
    }

    /// The subject over a snippet's unit (`slip_kiln::snippet`, for tests):
    /// a snippet's path is its subject path; a buffer the front end made
    /// itself is no file of the subject.
    /// Parsed with `slip_kiln::snippet::parse_with_tables`; `Err` as for
    /// [`Self::new`].
    pub fn from_snippet(
        p: &slip_kiln::snippet::Parsed<'u>,
        owned: &'u (dyn Fn(&[u8]) -> bool + Sync),
    ) -> Result<KilnSubject<'u>, String> {
        KilnSubject::from_parts(
            p.unit(),
            |id| p.path(id).map(|x| Cow::Owned(x.as_bytes().to_vec())),
            owned,
        )
    }

    /// The subject over `unit`, with `path`, the subject path of each of
    /// its buffers that is a file of the subject (asked once per buffer,
    /// here).
    fn from_parts(
        unit: &slip_kiln::Unit<'u>,
        path: impl Fn(FileId) -> Option<Cow<'u, [u8]>>,
        owned: &'u (dyn Fn(&[u8]) -> bool + Sync),
    ) -> Result<KilnSubject<'u>, String> {
        if !slip_kiln::written::recorded(unit) {
            return Err("the unit was parsed without Kiln's written-reference and \
                        declaration-extent tables (ParseOptions::written)"
                .to_owned());
        }
        let (ast, sm): (&'u Ast, &'u SourceManager) = (unit.ast(), unit.sm());
        let mut s = KilnSubject {
            ast,
            sm,
            paths: sm.file_ids().map(path).collect(),
            owned,
            files: (0..sm.file_count()).map(|_| OnceLock::new()).collect(),
            parm_owner: OnceLock::new(),
            scopes: Mutex::new(HashMap::new()),
            names: references::Names::default(),
        };
        s.names = references::Names::build(&s, unit);
        Ok(s)
    }

    /// The scope indexes (a poisoned lock is as good: an index is inserted
    /// whole or not at all).
    fn scopes(&self) -> MutexGuard<'_, HashMap<u32, Arc<lookup::Index>>> {
        self.scopes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The subject path of buffer `id`.
    fn path(&self, id: FileId) -> Option<&[u8]> {
        self.paths.get(id.index()).and_then(|p| p.as_deref())
    }

    /// The subject's number for entity `e`'s declaration that is the
    /// entity. For tests and for WP-085: `e` must be live.
    pub fn entity_number(&self, d: DeclId) -> Entity {
        Entity(self.entity(d).index())
    }

    /// The buffer of file `f` with its path and bytes, when `f` is a file
    /// of the subject.
    fn file(&self, f: File) -> Option<&SubjectFile<'u>> {
        let i = (f.0 as usize).checked_sub(1)?;
        let cell = self.files.get(i)?;
        cell.get_or_init(|| {
            let id = self.sm.file_ids().nth(i)?;
            let path = self.path(id)?.to_vec();
            let bytes = self.sm.original_bytes(id)?;
            u32::try_from(bytes.len()).ok()?;
            Some((path, bytes))
        })
        .as_ref()
    }

    /// The file handle of buffer `id`.
    fn file_number(id: FileId) -> File {
        File(id.index() as u32 + 1)
    }

    /// Unit order of two locations: `slip_kiln::ranges`' (WP-084).
    fn order(&self, a: Loc, b: Loc) -> std::cmp::Ordering {
        unit_order(self.sm, a, b)
    }

    fn describe_loc(&self, loc: Loc) -> String {
        match self.sm.line_col(loc) {
            Some(lc) => {
                let name = self.path(lc.file).map_or_else(
                    || self.sm.file_name(lc.file).to_owned(),
                    |p| String::from_utf8_lossy(p).into_owned(),
                );
                format!("{name}:{}:{}", lc.line, lc.col)
            }
            None => "<no location>".to_owned(),
        }
    }
}

impl Subject for KilnSubject<'_> {
    fn is_live(&self, v: Value) -> bool {
        match v {
            Value::Entity(Entity(n)) => self.decl(n).is_some_and(|d| self.is_entity(d)),
            Value::Declaration(Declaration(n)) => {
                self.decl(n).is_some_and(|d| self.is_declaration(d))
            }
            Value::Reference(r) => self.reference_is_live(r),
            Value::Expression(_) => false,
        }
    }

    fn file_len(&self, f: File) -> Option<u32> {
        self.file(f).map(|(_, b)| b.len() as u32)
    }

    fn kind_bits(&self, v: Value) -> u64 {
        v.sort_bit()
            | match v {
                Value::Entity(Entity(n)) => self.decl(n).map_or(0, |d| self.entity_kinds(d)),
                Value::Declaration(Declaration(n)) => {
                    if self.decl(n).is_some_and(|d| self.is_definition(d)) {
                        wire::KIND_DEFINITION
                    } else {
                        0
                    }
                }
                Value::Reference(_) | Value::Expression(_) => 0,
            }
    }

    fn lookup(&self, _global: bool, components: &[&[u8]]) -> Vec<Entity> {
        self.lookup_entities(components).into_iter().map(|d| Entity(d.index())).collect()
    }

    fn identifier(&self, v: Value) -> Option<Vec<u8>> {
        match v {
            Value::Entity(Entity(n)) | Value::Declaration(Declaration(n)) => {
                Some(self.decl(n).map(|d| self.identifier_of(d)).unwrap_or_default())
            }
            Value::Reference(r) => Some(self.reference_identifier(r)),
            Value::Expression(_) => None,
        }
    }

    fn display(&self, v: Value) -> String {
        match v {
            Value::Entity(Entity(n)) => match self.decl(n) {
                Some(d) => slip_digest::display_name(self.ast, d),
                None => format!("entity {n}"),
            },
            Value::Declaration(Declaration(n)) => match self.decl(n) {
                Some(d) => format!(
                    "a declaration of {} at {}",
                    slip_digest::display_name(self.ast, self.entity(d)),
                    self.describe_loc(self.ast.decls.get(d).loc())
                ),
                None => format!("declaration {n}"),
            },
            Value::Reference(r) => self.reference_display(r),
            Value::Expression(Expression(n)) => format!("expression {n}"),
        }
    }

    fn parent(&self, e: Entity) -> Option<Entity> {
        self.decl(e.0).and_then(|d| self.parent_decl(d)).map(|p| Entity(p.index()))
    }

    fn parameters(&self, v: Value) -> Option<Vec<Entity>> {
        let f = match v {
            // §6.4 rule 2: an entity's are those of its first declaration
            // in unit order (one Kiln made, for an entity with none written).
            Value::Entity(Entity(n)) => {
                let e = self.decl(n)?;
                self.function_of(self.first_written(e).unwrap_or(e))?
            }
            Value::Declaration(Declaration(n)) => self.function_of(self.decl(n)?)?,
            Value::Reference(_) | Value::Expression(_) => return None,
        };
        Some(self.parameters_of(f).into_iter().map(|p| Entity(p.index())).collect())
    }

    fn declarations(&self, e: Entity) -> Result<Vec<Declaration>, Errc> {
        let Some(d) = self.decl(e.0) else { return Ok(Vec::new()) };
        if self.entity_kinds(d) & wire::KIND_PARAMETER != 0 {
            return Err(Errc::Unsupported);
        }
        Ok(self.written_declarations(d).into_iter().map(|x| Declaration(x.index())).collect())
    }

    fn entity_of(&self, d: Declaration) -> Entity {
        self.decl(d.0).map_or(Entity(self.ast.tu.index()), |x| Entity(self.entity(x).index()))
    }

    fn origin(&self, e: Entity) -> Entity {
        self.decl(e.0).map_or(e, |d| Entity(self.origin_decl(d).index()))
    }

    fn references_to(&self, e: Entity) -> Vec<Reference> {
        self.references_to_entity(e)
    }

    fn referent(&self, r: Reference) -> Entity {
        self.referent_of(r)
    }

    fn name_range(&self, v: Value) -> Result<Option<Range>, Errc> {
        Ok(match v {
            Value::Reference(r) => self.reference_name_range(r),
            Value::Declaration(Declaration(n)) => {
                self.decl(n).and_then(|d| self.declaration_name_range(d))
            }
            Value::Entity(_) | Value::Expression(_) => None,
        })
    }

    fn extent(&self, v: Value) -> Result<Option<Range>, Errc> {
        Ok(match v {
            Value::Reference(r) => self.reference_extent(r),
            Value::Entity(_) | Value::Declaration(_) | Value::Expression(_) => None,
        })
    }

    fn text(&self, r: Range) -> &[u8] {
        match self.file(r.file) {
            Some((_, b)) => b.get(r.lo as usize..r.hi as usize).unwrap_or_default(),
            None => &[],
        }
    }

    fn owned(&self, f: File) -> bool {
        self.file(f).is_some_and(|(p, _)| (self.owned)(p))
    }

    fn calls_to(&self, e: Entity) -> Result<Vec<Expression>, Errc> {
        let k = self.decl(e.0).map_or(0, |d| self.entity_kinds(d));
        let template_of_function = self.decl(e.0).is_some_and(|d| {
            self.kind(d) == slip_kiln::kiln::ast::decl::DeclKind::FunctionTemplate
        });
        if k & wire::KIND_FUNCTION == 0 && !template_of_function {
            return Err(Errc::WrongKind);
        }
        Ok(Vec::new())
    }

    fn callee(&self, _x: Expression) -> Option<Entity> {
        None
    }

    fn callee_reference(&self, _x: Expression) -> Option<Reference> {
        None
    }

    fn arguments(&self, _x: Expression) -> Option<Vec<Expression>> {
        None
    }

    fn unresolved_named(&self, identifier: &[u8]) -> Option<u64> {
        Some(self.unresolved_spelled(identifier))
    }

    fn tiers(&self, r: Range) -> Option<[bool; 2]> {
        self.name_tiers(r)
    }

    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>> {
        self.decl(e.0).and_then(|d| self.qualified_decl(d))
    }

    fn references_at(&self, r: Range) -> Vec<Reference> {
        self.references_with_extent(r)
    }

    fn found_directly(&self, r: Reference) -> bool {
        self.reference_found_directly(r)
    }

    fn entity_key(&self, _e: Entity) -> Option<slip_model::Digest> {
        None
    }

    fn unit_keys(&self, _v: Value) -> Result<Vec<slip_model::Digest>, Errc> {
        Err(Errc::Unsupported)
    }

    fn rename_rule(&self, _e: Entity, _new_identifier: &[u8]) -> Result<Rule, Errc> {
        Err(Errc::Unsupported)
    }

    fn place(&self, v: Value) -> PlaceOf {
        match v {
            // §7.4 rule 3: an entity's is its first written declaration's.
            Value::Entity(Entity(n)) => match self.decl(n).and_then(|d| self.first_written(d)) {
                Some(d) => self.declaration_place(d),
                None => PlaceOf::Nowhere,
            },
            Value::Declaration(Declaration(n)) => match self.decl(n) {
                Some(d) => self.declaration_place(d),
                None => PlaceOf::Unnamed,
            },
            Value::Reference(r) => self.reference_place(r),
            Value::Expression(_) => PlaceOf::Unnamed,
        }
    }

    fn file_ref(&self, f: File) -> FileRef {
        match self.file(f) {
            Some((path, bytes)) => FileRef {
                path: path.clone(),
                size: bytes.len() as u64,
                content: slip_model::hash::content_digest(bytes),
            },
            None => FileRef {
                path: Vec::new(),
                size: 0,
                content: slip_model::hash::content_digest(&[]),
            },
        }
    }
}
