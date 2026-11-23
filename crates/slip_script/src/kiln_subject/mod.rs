//! [`KilnSubject`]: the [`Subject`] over one parsed unit of Kiln
//! (`spec/api.md` §5, §6; `docs/wp/WP-083.md`).
//!
//! WP-083 answers entities, lookup and structure: `is_live`,
//! `kind_bits`, `lookup`, `identifier`, `display`, `parent`, `parameters`,
//! `declarations`, `entity_of`, `origin`, `qualified`, `file_len`, `text`,
//! `owned`, `file_ref`. WP-086 (`calls.rs`) answers calls and the extents
//! of expressions: `calls_to`, `callee`, `callee_reference`, `arguments`,
//! and `extent`, `tiers` and `place` of an expression; the live
//! expressions are the calls and their arguments, and the live references
//! (until WP-085) the table's entries in a role of §6.5 rule 3. Every
//! other method answers "nothing" as the trait documents it, until WP-085
//! (references and ranges) and WP-095 (keys).
//!
//! Everything is read from Kiln's declaration arena; what is costly is
//! built on first use and kept for the life of the subject: the set of
//! listed declarations (`entities.rs`), one name index per scope that
//! `lookup` reaches (`lookup.rs`), each file's bytes.

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

mod calls;
mod entities;
mod lookup;
mod order;

pub use calls::{CallSite, ExtentCensus, NoExtent};
pub use order::unit_order;

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
    /// The declarations reached through member lists (`entities.rs`).
    listed: OnceLock<Vec<u64>>,
    /// Template parameter → its template (`entities.rs`).
    parm_owner: OnceLock<HashMap<u32, u32>>,
    /// The name index of each scope `lookup` reached (`lookup.rs`). The
    /// lock is held only to read or insert, never while an index is
    /// built: two threads may build one index twice, to the same value.
    scopes: Mutex<HashMap<u32, Arc<lookup::Index>>>,
    /// The unit's tokens, for the end of a token (`calls.rs`).
    tokens: slip_kiln::Tokens<'u>,
    /// The unit's written references, read when the subject is made (the
    /// unit is not kept); a reference is numbered by its index here, plus
    /// one (`calls.rs`).
    refs: Vec<slip_kiln::written::WrittenRef>,
    /// The indexes in `refs` of the entries whose name starts at a
    /// location, by the location (`calls.rs`).
    ref_names: OnceLock<HashMap<u32, Vec<u32>>>,
    /// The calls of the unit (`calls.rs`).
    calls: OnceLock<calls::Index>,
    /// The tiers of each extent of a live expression (`calls.rs`).
    call_tiers: OnceLock<HashMap<Range, [bool; 2]>>,
}

impl std::fmt::Debug for KilnSubject<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KilnSubject")
            .field("declarations", &self.ast.decls.len())
            .field("scopes_indexed", &self.scopes().len())
            .finish_non_exhaustive()
    }
}

impl<'u> KilnSubject<'u> {
    /// `owned`: the run's ownership, by subject file (its real path).
    /// The subject keeps the unit's AST, source manager and real paths,
    /// not the unit (which holds the Oven, and is not `Sync`).
    pub fn new(
        unit: &'u slip_kiln::Unit<'u>,
        owned: &'u (dyn Fn(&[u8]) -> bool + Sync),
    ) -> KilnSubject<'u> {
        KilnSubject::from_parts(unit, |f| unit.real_path(f).map(Cow::Borrowed), owned)
    }

    /// The subject over a snippet's unit (`slip_kiln::snippet`, for tests):
    /// a snippet's path is its subject path; a buffer the front end made
    /// itself is no file of the subject.
    pub fn from_snippet(
        p: &'u slip_kiln::snippet::Parsed<'u>,
        owned: &'u (dyn Fn(&[u8]) -> bool + Sync),
    ) -> KilnSubject<'u> {
        KilnSubject::from_parts(
            p.unit(),
            |id| p.path(id).map(|x| Cow::Owned(x.as_bytes().to_vec())),
            owned,
        )
    }

    /// The subject over a unit and `path`, the subject path of each of
    /// the unit's buffers that is a file of the subject (asked once per
    /// buffer, here). It keeps the unit's AST, source manager, tokens and
    /// written references, not the unit.
    fn from_parts(
        unit: &'u slip_kiln::Unit<'u>,
        path: impl Fn(FileId) -> Option<Cow<'u, [u8]>>,
        owned: &'u (dyn Fn(&[u8]) -> bool + Sync),
    ) -> KilnSubject<'u> {
        let (ast, sm) = (unit.ast(), unit.sm());
        KilnSubject {
            ast,
            sm,
            paths: sm.file_ids().map(path).collect(),
            owned,
            files: (0..sm.file_count()).map(|_| OnceLock::new()).collect(),
            listed: OnceLock::new(),
            parm_owner: OnceLock::new(),
            scopes: Mutex::new(HashMap::new()),
            tokens: unit.tokens(),
            refs: slip_kiln::written::written_refs(unit),
            ref_names: OnceLock::new(),
            calls: OnceLock::new(),
            call_tiers: OnceLock::new(),
        }
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
    #[allow(dead_code, reason = "the ranges of WP-085 give files out")]
    fn file_number(id: FileId) -> File {
        File(id.index() as u32 + 1)
    }

    /// Unit order of two locations: the one place the subject compares
    /// positions, so that WP-084's comparator replaces [`unit_order`] here.
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
            Value::Reference(Reference(n)) => self.reference_live(n),
            Value::Expression(Expression(n)) => self.expression_live(n),
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
                Value::Expression(Expression(n)) if self.is_call(n) => wire::KIND_CALL,
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
            Value::Reference(_) => Some(Vec::new()),
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
            Value::Reference(Reference(n)) => format!("reference {n}"),
            Value::Expression(Expression(n)) => self.expression_display(n),
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

    fn references_to(&self, _e: Entity) -> Vec<Reference> {
        Vec::new()
    }

    fn referent(&self, r: Reference) -> Entity {
        Entity(self.reference_referent(r.0).unwrap_or(self.ast.tu).index())
    }

    fn name_range(&self, _v: Value) -> Result<Option<Range>, Errc> {
        Ok(None)
    }

    fn extent(&self, v: Value) -> Result<Option<Range>, Errc> {
        Ok(match v {
            Value::Expression(Expression(n)) => self.expression_extent(n),
            Value::Entity(_) | Value::Declaration(_) | Value::Reference(_) => None,
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
        Ok(self.decl(e.0).map_or_else(Vec::new, |d| self.calls_of(self.origin_decl(d))))
    }

    fn callee(&self, x: Expression) -> Option<Entity> {
        self.callee_of(x).map(|d| Entity(d.index()))
    }

    fn callee_reference(&self, x: Expression) -> Option<Reference> {
        self.callee_reference_of(x)
    }

    fn arguments(&self, x: Expression) -> Option<Vec<Expression>> {
        self.arguments_of(x)
    }

    fn unresolved_named(&self, _identifier: &[u8]) -> Option<u64> {
        None
    }

    fn tiers(&self, r: Range) -> Option<[bool; 2]> {
        self.expression_tiers(r)
    }

    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>> {
        self.decl(e.0).and_then(|d| self.qualified_decl(d))
    }

    fn references_at(&self, _r: Range) -> Vec<Reference> {
        Vec::new()
    }

    fn found_directly(&self, _r: Reference) -> bool {
        false
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
            Value::Entity(Entity(n)) => match self.decl(n) {
                Some(d) if self.first_written(d).is_some() => PlaceOf::Unnamed,
                _ => PlaceOf::Nowhere,
            },
            Value::Expression(Expression(n)) => self.expression_place(n),
            Value::Declaration(_) | Value::Reference(_) => PlaceOf::Unnamed,
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
