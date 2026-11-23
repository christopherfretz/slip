//! References, their ranges and places, and the written declarations, over
//! Kiln's written-reference and declaration-extent tables
//! (`spec/api.md` §5.4, §6.5, §6.6, §7.4 rule 3; `docs/wp/WP-085.md`;
//! `spec/landed/api/WP-085.md` has the decisions).
//!
//! **Everything that needs the unit is computed when the subject is
//! made.** Kiln's end of a token and its token starts are `Unit`'s
//! (`Unit::token_end`, `Unit::is_token_start`), and the subject does not
//! keep the unit (it is not `Sync`, WP-083 decision 17). So [`Names::build`]
//! reads both tables once, through `slip_kiln::written`, and turns every
//! entry into a reference with its ranges and place; what needs only the
//! subject (the index by origin, the sort into unit order, the index by
//! extent) is built on first use and kept.
//!
//! **No range of its own.** Every range is `slip_kiln::ranges`'
//! `file_range_of_span` of a span of Kiln's tokens (WP-084). This module
//! only decides which span to ask about and refuses answers: a name range
//! that is not the identifier token itself, a range in a file Kiln
//! recoded or in a buffer that is no file of the subject.
//!
//! **One written name is one reference.** Entries equal in role,
//! referent, where the name is spelled, the macro invocation it came
//! through, its name range, its extent and whether it was found directly
//! are one reference: an argument used twice by a macro, a header entered
//! twice, an X-macro file read under two definitions give one reference
//! when they give one name to edit (`spec/landed/api/WP-085.md`
//! decision 3).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use slip_kiln::Unit;
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::source::{FileId, Loc, Span};
use slip_kiln::ranges::{self, FileRange, Tier};
use slip_kiln::written::{self, DeclNameKind, Role, WrittenRef};

use super::KilnSubject;
use crate::subject::{Entity, PlaceOf, Range, Reference};

/// A range given out (by `name_range` or `extent`), with its tier.
type Given = (Range, Tier);

/// What the subject knows of the names of its unit: built once, when the
/// subject is made ([`Names::build`]), and indexed on first use.
#[derive(Default)]
pub(super) struct Names {
    /// One bit per declaration: it has a row in the declaration-extent
    /// table that is not an explicit instantiation's.
    written: Vec<u64>,
    /// The references, numbered by position.
    refs: Vec<Ref>,
    /// The name range of each declaration `decl_names` gives, and the
    /// nearest place when it has none.
    decls: HashMap<u32, Place>,
    /// Every range given out, and the tiers at which the unit reaches it.
    tiers: HashMap<Range, [bool; 2]>,
    /// The number of unresolved written names spelled so, each written
    /// name once (§6.5 rule 6).
    unresolved: HashMap<Vec<u8>, u64>,
    /// The reference of each entry, by the raw start of its name.
    at_loc: HashMap<u32, u32>,
    /// Origin (an entity's number) → its references, in table order.
    by_origin: OnceLock<HashMap<u32, Vec<u32>>>,
    /// `references_to` answers already sorted into unit order.
    sorted: Mutex<HashMap<u32, Arc<[Reference]>>>,
    /// Extent → the references that have it.
    by_extent: OnceLock<HashMap<Range, Vec<Reference>>>,
}

impl std::fmt::Debug for Names {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Names")
            .field("references", &self.refs.len())
            .field("declaration_names", &self.decls.len())
            .field("ranges_given", &self.tiers.len())
            .field("unresolved_spellings", &self.unresolved.len())
            .field("bytes_about", &self.bytes())
            .finish_non_exhaustive()
    }
}

impl Names {
    /// About how many bytes the tables built when the subject was made
    /// hold (the measurement of `tests/subject_references.rs`).
    fn bytes(&self) -> usize {
        use std::mem::size_of;
        self.written.len() * 8
            + self.refs.capacity() * size_of::<Ref>()
            + self.decls.capacity() * (size_of::<u32>() + size_of::<Place>() + 8)
            + self.tiers.capacity() * (size_of::<Range>() + 2 + 8)
            + self.at_loc.capacity() * (8 + 8)
            + self.unresolved.keys().map(|k| k.len() + 40).sum::<usize>()
    }
}

/// One reference.
struct Ref {
    /// The entity the name denotes (Kiln's referent, as the subject
    /// numbers entities).
    referent: DeclId,
    /// The first token of the written name (its outermost qualifier
    /// component; a leading `::` is just before it), for unit order: the
    /// earliest of the entries joined into this reference.
    first: Loc,
    /// The end of the name, for "outermost first".
    end: Loc,
    name: Option<Given>,
    extent: Option<Given>,
    /// The nearest place when there is no extent (§7.4 rule 3).
    nearest: Option<Range>,
    /// Lookup did not go through a using-declaration (§7.2 rule 1).
    direct: bool,
}

/// The name range of a declaration, or the nearest place.
struct Place {
    name: Option<Given>,
    nearest: Option<Range>,
}

/// What makes two entries one reference (module documentation).
#[derive(PartialEq, Eq, Hash)]
struct Key {
    role: u8,
    referent: u32,
    spelled: Option<(FileId, u32)>,
    invocation: Option<Range>,
    name: Option<Range>,
    extent: Option<Range>,
    direct: bool,
}

/// The roles of §6.5 rule 3. Kiln's other roles (stage 3: using-
/// declarations and -directives, namespace alias targets, elaborated and
/// friend references, specialisation targets, concept names, designators)
/// are §6.5 rule 4's and are skipped until their feature test is defined.
fn returned_role(role: Role) -> bool {
    matches!(
        role,
        Role::Expr
            | Role::Member
            | Role::Nns
            | Role::Type
            | Role::Template
            | Role::Base
            | Role::MemInit
    )
}

impl Names {
    /// Reads the unit's tables (module documentation). `s` is the subject
    /// being made, without its names yet: only what needs no name is
    /// asked of it.
    pub(super) fn build(s: &KilnSubject<'_>, unit: &Unit<'_>) -> Names {
        let mut names = Names {
            written: written_bits(unit),
            decls: decl_places(s, unit),
            unresolved: unresolved_counts(unit),
            ..Names::default()
        };
        names.add_references(s, unit);
        for p in names.decls.values() {
            if let Some(g) = p.name {
                names.tiers.entry(g.0).or_insert([false, false])[g.1 as usize] = true;
            }
        }
        names
    }

    /// The references of §6.5 rule 3, one per written name (module
    /// documentation).
    fn add_references(&mut self, s: &KilnSubject<'_>, unit: &Unit<'_>) {
        let sm = unit.sm();
        let declarative = Declarative::build(unit);
        let mut keys: HashMap<Key, u32> = HashMap::new();
        for r in written::written_refs(unit) {
            if !returned_role(r.role) || declarative.holds(unit, &r) {
                continue;
            }
            let referent = s.entity(r.referent);
            if !s.is_entity(referent) {
                continue;
            }
            let x = s.make_ref(unit, &r, referent);
            let key = Key {
                role: r.role as u8,
                referent: referent.index(),
                spelled: sm.offset_of(sm.spelling_loc(r.name.lo)),
                invocation: s.invocation(r.name.lo),
                name: x.name.map(|g| g.0),
                extent: x.extent.map(|g| g.0),
                direct: x.direct,
            };
            let at = if let Some(&i) = keys.get(&key) {
                let y = &mut self.refs[i as usize];
                if super::unit_order(sm, x.first, y.first) == Ordering::Less {
                    y.first = x.first;
                    y.end = x.end;
                }
                i
            } else {
                let i = self.refs.len() as u32;
                keys.insert(key, i);
                for g in [x.name, x.extent].into_iter().flatten() {
                    self.tiers.entry(g.0).or_insert([false, false])[g.1 as usize] = true;
                }
                self.refs.push(x);
                i
            };
            self.at_loc.entry(r.name.lo.to_raw()).or_insert(at);
        }
    }
}

/// Written declarations: a row that is not an explicit instantiation's
/// (WP-083 decision 2), one bit per declaration.
fn written_bits(unit: &Unit<'_>) -> Vec<u64> {
    let ast = unit.ast();
    let mut bits = vec![0u64; ast.decls.len() / 64 + 2];
    for (d, _) in ast.decl_extents.iter() {
        if written::row(unit, d).is_some_and(|k| k != DeclNameKind::ExplicitInstantiation) {
            let i = d.index() as usize;
            if let Some(w) = bits.get_mut(i / 64) {
                *w |= 1 << (i % 64);
            }
        }
    }
    bits
}

/// The name range of each declaration `decl_names` gives (§6.6 rule 1),
/// or its nearest place.
fn decl_places(s: &KilnSubject<'_>, unit: &Unit<'_>) -> HashMap<u32, Place> {
    let mut out = HashMap::new();
    for n in written::decl_names(unit) {
        let ident = s.identifier_of(n.decl);
        let fr = ranges::file_range_of_span(unit.sm(), n.name);
        let name = s.given(fr.filter(|&fr| identifier_token(unit, fr, n.name, &ident)));
        let nearest = match name {
            Some(_) => None,
            None => s.given(fr).map(|g| g.0).or_else(|| s.invocation(n.name.lo)),
        };
        out.insert(n.decl.index(), Place { name, nearest });
    }
    out
}

/// §6.5 rule 6: the unresolved written names by spelling, each once: by
/// where its token is spelled.
fn unresolved_counts(unit: &Unit<'_>) -> HashMap<Vec<u8>, u64> {
    let sm = unit.sm();
    let mut out: HashMap<Vec<u8>, u64> = HashMap::new();
    let mut seen = HashSet::new();
    for n in written::dependent_names(unit) {
        let Some(spelled) = sm.offset_of(sm.spelling_loc(n.name.lo)) else { continue };
        if !seen.insert(spelled) {
            continue;
        }
        if let Some(t) = sm.span_text(n.name) {
            *out.entry(t.as_bytes().to_vec()).or_insert(0) += 1;
        }
    }
    out
}

/// The spans of the declarative contexts of §6.5 rule 4 whose names Kiln
/// records in a role of rule 3: the qualifier of a declarator-id (and of
/// a using-declaration, a using-directive's namespace, an explicit
/// specialisation's name: `DeclExtent::qual` to the name), and the whole
/// of a using-declaration, a using-directive and a namespace alias.
struct Declarative {
    /// Spans within one entry, as raw `[lo, hi)`, sorted and joined.
    raw: Vec<(u32, u32)>,
    /// Spans whose ends are in different entries, compared by unit order.
    other: Vec<Span>,
}

impl Declarative {
    fn build(unit: &Unit<'_>) -> Declarative {
        let ast = unit.ast();
        let sm = unit.sm();
        let mut spans = Vec::new();
        for (d, row) in ast.decl_extents.iter() {
            if row.qual.is_valid() && row.name.lo.is_valid() {
                spans.push(Span::new(row.qual, row.name.lo));
            }
            let whole = matches!(
                ast.decls.get(d).kind(),
                DeclKind::Using
                    | DeclKind::UsingPack
                    | DeclKind::UnresolvedUsingValue
                    | DeclKind::UnresolvedUsingTypename
                    | DeclKind::UnresolvedUsingIfExists
                    | DeclKind::UsingDirective
                    | DeclKind::NamespaceAlias
            );
            if whole && row.full.is_valid() {
                spans.push(row.full);
            }
        }
        let mut raw = Vec::new();
        let mut other = Vec::new();
        for s in spans {
            match sm.same_entry_distance(s.lo, s.hi) {
                Some(_) => raw.push((s.lo.to_raw(), s.hi.to_raw())),
                None => other.push(s),
            }
        }
        raw.sort_unstable();
        let mut joined: Vec<(u32, u32)> = Vec::with_capacity(raw.len());
        for (lo, hi) in raw {
            match joined.last_mut() {
                Some(last) if lo <= last.1 => last.1 = last.1.max(hi),
                _ => joined.push((lo, hi)),
            }
        }
        Declarative { raw: joined, other }
    }

    /// Whether the name of `r` is in one of the spans: its first token,
    /// or the macro invocation it came from.
    fn holds(&self, unit: &Unit<'_>, r: &WrittenRef) -> bool {
        let sm = unit.sm();
        let at = r.name.lo;
        let outer = sm.expansion_loc(at);
        let in_raw = |l: Loc| {
            let raw = l.to_raw();
            let k = self.raw.partition_point(|&(lo, _)| lo <= raw);
            k > 0 && raw < self.raw[k - 1].1
        };
        if in_raw(at) || (outer != at && in_raw(outer)) {
            return true;
        }
        self.other.iter().any(|s| {
            super::unit_order(sm, s.lo, at) != Ordering::Greater
                && super::unit_order(sm, at, s.hi) == Ordering::Less
        })
    }
}

/// §6.6 rule 1: `fr`, the range of the span `name`, is the identifier
/// token itself: it starts where the name's token is spelled, Kiln's end
/// of the token there is its end, and its text is the identifier
/// `ident` (empty: the name is not an identifier). Not so for a name that
/// is a whole macro expansion (`ID(foo)`, `#define N n`), whose range is
/// the invocation, nor for a name written with a line splice or a
/// universal-character-name.
fn identifier_token(unit: &Unit<'_>, fr: FileRange, name: Span, ident: &[u8]) -> bool {
    let sm = unit.sm();
    let start = sm.loc(fr.file, fr.lo);
    !ident.is_empty()
        && sm.offset_of(sm.spelling_loc(name.lo)) == Some((fr.file, fr.lo))
        && unit.token_end(start) == Some(sm.loc(fr.file, fr.hi))
        && sm.text(fr.file).as_bytes().get(fr.lo as usize..fr.hi as usize) == Some(ident)
}

/// §6.6 rule 2: the range of the written name of `r`, from its first token
/// (a leading `::`, or the `~` of an unqualified destructor name, which
/// the table does not place) to the end of its name; `None` when that
/// token is not found exactly.
fn extent_span(unit: &Unit<'_>, r: &WrittenRef, destructor: bool) -> Option<FileRange> {
    let start = if r.global {
        global_start(unit, r)?
    } else if destructor && r.first == r.name.lo {
        match token_before(unit, r.name.lo)? {
            (at, "~" | "compl") => at,
            _ => return None,
        }
    } else {
        r.first
    };
    ranges::file_range_of_span(unit.sm(), Span::new(start, r.name.hi))
}

/// The location of the leading `::` of `r` (`docs/wp/WP-085.md` item 5):
/// the token just before the first component, where that component is
/// spelled, in the component's own entry; or, when the name's range
/// starts at a macro invocation in file text that produced the first
/// component, the token just before that invocation. Its tier is then
/// `ranges`' answer for the whole span.
fn global_start(unit: &Unit<'_>, r: &WrittenRef) -> Option<Loc> {
    if let Some((at, "::")) = token_before(unit, r.first) {
        return Some(at);
    }
    let sm = unit.sm();
    if r.first.is_macro() {
        let e = ranges::file_range_of_span(sm, Span::new(r.first, r.name.hi))?;
        let outer = sm.expansion_loc(r.first);
        if e.tier == Tier::File
            && sm.offset_of(outer) == Some((e.file, e.lo))
            && let Some((at, "::")) = token_before(unit, outer)
        {
            return Some(at);
        }
    }
    None
}

/// The token just before the token at `at`, on the line where `at` is
/// spelled, as Kiln's lexer lexes that file ([`Unit::is_token_start`],
/// [`Unit::token_end`]): its location in `at`'s own entry and its text.
/// `None` when it is on another line or outside `at`'s entry.
fn token_before<'u>(unit: &Unit<'u>, at: Loc) -> Option<(Loc, &'u str)> {
    let sm = unit.sm();
    let (file, off) = sm.offset_of(sm.spelling_loc(at))?;
    let text = sm.text(file).as_bytes();
    let line = text.get(..off as usize)?.iter().rposition(|&c| c == b'\n').map_or(0, |k| k + 1);
    let line = u32::try_from(line).ok()?;
    let k = (line..off).rev().find(|&k| unit.is_token_start(sm.loc(file, k)))?;
    let back = off - k;
    let before = Loc::from_raw(at.to_raw().checked_sub(back)?);
    if sm.same_entry_distance(before, at) != Some(back) {
        return None;
    }
    let end = unit.token_end(before)?;
    Some((before, sm.span_text(Span::new(before, end))?))
}

/// Whether `inner` lies in `outer`.
fn within(inner: Range, outer: Range) -> bool {
    inner.file == outer.file && outer.lo <= inner.lo && inner.hi <= outer.hi
}

impl KilnSubject<'_> {
    /// A file range of Kiln's as a range of the subject, when its buffer
    /// is a file of the subject that Kiln did not recode on loading
    /// (`spec/edits.md` §2.3, `file_recoded`: then Kiln's offsets are the
    /// file's bytes').
    fn given(&self, fr: Option<FileRange>) -> Option<Given> {
        let fr = fr?;
        self.path(fr.file)?;
        if !self.sm.load_report(fr.file).is_clean() {
            return None;
        }
        Some((Range { file: Self::file_number(fr.file), lo: fr.lo, hi: fr.hi }, fr.tier))
    }

    /// The outermost macro invocation that produced `at`, written in a
    /// file (§7.4 rule 3, "the nearest place"); `None` for a file
    /// location.
    fn invocation(&self, at: Loc) -> Option<Range> {
        if !at.is_macro() {
            return None;
        }
        let span = self.sm.expansion_span(at);
        self.given(ranges::file_range_of_span(self.sm, span)).map(|g| g.0)
    }

    /// The reference an entry gives, before it is joined with others.
    fn make_ref(&self, unit: &Unit<'_>, r: &WrittenRef, referent: DeclId) -> Ref {
        let ident = self.identifier_of(referent);
        let fr = ranges::file_range_of_span(self.sm, r.name);
        let mut name = self.given(fr.filter(|&fr| identifier_token(unit, fr, r.name, &ident)));
        let destructor = self.kind(referent) == DeclKind::CXXDestructor;
        let extent = self.given(extent_span(unit, r, destructor));
        if let (Some(n), Some(x)) = (name, extent)
            && !within(n.0, x.0)
        {
            // Never seen; a name range outside its extent would make the
            // sibling rule fail, so the name has none.
            debug_assert!(false, "a name range outside its extent");
            name = None;
        }
        let nearest = match extent {
            Some(_) => None,
            None => self
                .given(ranges::file_range_of_span(self.sm, Span::new(r.first, r.name.hi)))
                .or(name)
                .map(|g| g.0)
                .or_else(|| self.invocation(r.name.lo)),
        };
        Ref {
            referent,
            first: r.first,
            end: r.name.hi,
            name,
            extent,
            nearest,
            direct: !r.via_using,
        }
    }

    fn names(&self) -> &Names {
        &self.names
    }

    fn reference(&self, r: Reference) -> Option<&Ref> {
        self.names().refs.get(r.0 as usize)
    }

    /// Whether `d` is a declaration written in the unit (§6.4 rule 3): it
    /// has a row in Kiln's declaration-extent table (Kiln `spec/ast.md`
    /// §19.1: one per declaration the parser parsed, none for an implicit
    /// declaration, an instantiation or its members), read through
    /// `slip_kiln::written::row`, and the row is not an explicit
    /// instantiation's (WP-083 decision 2).
    pub(super) fn written_by_table(&self, d: DeclId) -> bool {
        let i = d.index() as usize;
        self.names().written.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1)
    }

    pub(super) fn reference_is_live(&self, r: Reference) -> bool {
        self.reference(r).is_some()
    }

    pub(super) fn reference_identifier(&self, r: Reference) -> Vec<u8> {
        self.reference(r).map(|x| self.identifier_of(x.referent)).unwrap_or_default()
    }

    pub(super) fn reference_display(&self, r: Reference) -> String {
        match self.reference(r) {
            Some(x) => format!(
                "a reference to {} at {}",
                slip_digest::display_name(self.ast, x.referent),
                self.describe_loc(x.first)
            ),
            None => format!("reference {}", r.0),
        }
    }

    /// §6.5 rule 2.
    pub(super) fn referent_of(&self, r: Reference) -> Entity {
        self.reference(r).map_or(Entity(self.ast.tu.index()), |x| Entity(x.referent.index()))
    }

    /// §6.5 rule 1: the references whose referent's origin (§5.5, the
    /// subject's `origin`) is `e`, in unit order (outermost first).
    pub(super) fn references_to_entity(&self, e: Entity) -> Vec<Reference> {
        let names = self.names();
        let by_origin = names.by_origin.get_or_init(|| {
            let mut map: HashMap<u32, Vec<u32>> = HashMap::new();
            for (i, x) in names.refs.iter().enumerate() {
                map.entry(self.origin_decl(x.referent).index()).or_default().push(i as u32);
            }
            map
        });
        let Some(bucket) = by_origin.get(&e.0) else { return Vec::new() };
        let lock = || names.sorted.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(done) = lock().get(&e.0) {
            return done.to_vec();
        }
        let mut sorted = bucket.clone();
        sorted.sort_by(|&a, &b| {
            let (x, y) = (&names.refs[a as usize], &names.refs[b as usize]);
            super::unit_order(self.sm, x.first, y.first)
                .then_with(|| super::unit_order(self.sm, y.end, x.end))
                .then(a.cmp(&b))
        });
        let done: Arc<[Reference]> = sorted.into_iter().map(Reference).collect();
        lock().insert(e.0, Arc::clone(&done));
        done.to_vec()
    }

    /// §6.6 rule 1 of a reference.
    pub(super) fn reference_name_range(&self, r: Reference) -> Option<Range> {
        self.reference(r).and_then(|x| x.name).map(|g| g.0)
    }

    /// §6.6 rule 1 of a declaration.
    pub(super) fn declaration_name_range(&self, d: DeclId) -> Option<Range> {
        self.names().decls.get(&d.index()).and_then(|p| p.name).map(|g| g.0)
    }

    /// §6.6 rules 2 and 4 of a reference.
    pub(super) fn reference_extent(&self, r: Reference) -> Option<Range> {
        self.reference(r).and_then(|x| x.extent).map(|g| g.0)
    }

    /// §7.2 rule 1: the references whose extent is `r`.
    pub(super) fn references_with_extent(&self, r: Range) -> Vec<Reference> {
        let names = self.names();
        let map = names.by_extent.get_or_init(|| {
            let mut map: HashMap<Range, Vec<Reference>> = HashMap::new();
            for (i, x) in names.refs.iter().enumerate() {
                if let Some(g) = x.extent {
                    map.entry(g.0).or_default().push(Reference(i as u32));
                }
            }
            map
        });
        map.get(&r).cloned().unwrap_or_default()
    }

    /// §7.2 rule 1, third condition.
    pub(super) fn reference_found_directly(&self, r: Reference) -> bool {
        self.reference(r).is_some_and(|x| x.direct)
    }

    /// §5.4, §7.3 rule 2: the tiers at which the unit reaches `r`, when a
    /// name range or an extent of a reference or a declaration is `r`.
    pub(super) fn name_tiers(&self, r: Range) -> Option<[bool; 2]> {
        self.names().tiers.get(&r).copied()
    }

    /// §6.5 rule 6.
    pub(super) fn unresolved_spelled(&self, identifier: &[u8]) -> u64 {
        self.names().unresolved.get(identifier).copied().unwrap_or(0)
    }

    /// The reference of the entry whose name starts at `at`, for the
    /// callee of a call (WP-086).
    #[allow(dead_code, reason = "for WP-086's callee_reference")]
    pub(super) fn reference_at(&self, at: Loc) -> Option<Reference> {
        self.names().at_loc.get(&at.to_raw()).map(|&i| Reference(i))
    }

    /// §7.4 rule 3 of a reference.
    pub(super) fn reference_place(&self, r: Reference) -> PlaceOf {
        match self.reference(r) {
            Some(x) => place(x.extent.map(|g| g.0), x.nearest),
            None => PlaceOf::Unnamed,
        }
    }

    /// §7.4 rule 3 of a declaration: its name; else the nearest place
    /// (its name's tokens, the macro invocation that produced them).
    pub(super) fn declaration_place(&self, d: DeclId) -> PlaceOf {
        if let Some(p) = self.names().decls.get(&d.index()) {
            return place(p.name.map(|g| g.0), p.nearest);
        }
        // A declaration `decl_names` does not give (an operator, a
        // conversion function, an unnamed declaration): its name's tokens
        // as the extent table records them, or its location's invocation.
        let named = self.ast.decl_extent(d).map(|x| x.name).filter(|n| n.lo.is_valid());
        let nearest = named
            .and_then(|n| self.given(ranges::file_range_of_span(self.sm, n)).map(|g| g.0))
            .or_else(|| self.invocation(self.ast.decls.get(d).loc()));
        place(None, nearest)
    }
}

fn place(own: Option<Range>, nearest: Option<Range>) -> PlaceOf {
    match (own, nearest) {
        (Some(r), _) => PlaceOf::Own(r),
        (None, Some(r)) => PlaceOf::Nearest(r),
        (None, None) => PlaceOf::Unnamed,
    }
}

/// WP-085's comparison of WP-083's structural rule for "written" with the
/// table's answer, over every snippet of the digest's pair tests, both
/// sides (`crates/slip_kiln/tests/ranges.rs` builds them the same way).
/// Each disagreement is counted by kind and printed with its place; the
/// findings are in `spec/landed/api/WP-085.md`. Deleted with the rule.
#[cfg(test)]
mod compare_written {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use slip_kiln::snippet::{Extra, Snippet, parse_with_tables};

    use super::*;

    fn pair_dirs(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            let mut mains = false;
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if matches!(e.file_name().to_str(), Some("t.cpp" | "a.cpp" | "b.cpp")) {
                    mains = true;
                }
            }
            if mains {
                out.push(d);
            }
        }
        out.sort();
        out
    }

    struct Side {
        files: Vec<(String, String)>,
        std: String,
        args: Vec<String>,
        target: Option<String>,
    }

    fn sides(dir: &Path, support: &[(String, String)]) -> Vec<Side> {
        let mut all: BTreeMap<String, String> = BTreeMap::new();
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.path().is_file()
                && let (Some(n), Ok(t)) =
                    (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
            {
                all.insert(n, t);
            }
        }
        let mut out = Vec::new();
        for side in ["a", "b"] {
            let mut files = Vec::new();
            let main = if let Some(t) = all.get("t.cpp") {
                files.push((
                    "side.h".to_owned(),
                    if side == "a" { "" } else { "#define B 1\n" }.into(),
                ));
                t.clone()
            } else if let Some(m) = all.get(&format!("{side}.cpp")) {
                m.clone()
            } else {
                continue;
            };
            files.insert(0, ("main.cpp".to_owned(), main.clone()));
            for (n, t) in &all {
                if !matches!(n.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
                    files.push((n.clone(), t.clone()));
                }
            }
            for (n, t) in support {
                if !all.contains_key(n) {
                    files.push((n.clone(), t.clone()));
                }
            }
            let mut s = Side { files, std: "c++20".to_owned(), args: Vec::new(), target: None };
            for line in main.lines() {
                let line = line.trim_end();
                if line == "#include \"side.h\"" {
                    continue;
                }
                let Some((key, value)) = line.strip_prefix("// ").and_then(|r| r.split_once(": "))
                else {
                    break;
                };
                match key {
                    "std" => value.clone_into(&mut s.std),
                    "args" => {
                        s.args =
                            value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect();
                    }
                    "target" => s.target = Some(value.to_owned()),
                    "errors" => {}
                    _ => break,
                }
            }
            out.push(s);
        }
        out
    }

    /// (label, examples) per (kind, structural, table, a declaration of
    /// the subject by the other tests of `is_declaration`).
    type Tally = BTreeMap<(String, bool, bool, bool), (u64, Vec<String>)>;

    fn compare(side: &Side, label: &str, tally: &mut Tally) -> u64 {
        let files: Vec<(&str, &str)> =
            side.files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        let snippet = Snippet { files: &files, main: "main.cpp", include: &[""], std: &side.std };
        let args: Vec<&str> = side.args.iter().map(String::as_str).collect();
        let extra = Extra { args: &args, target: side.target.as_deref() };
        let owned = |_: &[u8]| false;
        parse_with_tables(&snippet, &extra, |p| {
            let s = KilnSubject::from_snippet(p, &owned).expect("tables");
            let mut count = 0;
            for i in 1..=p.ast.decls.len() as u32 {
                let d = DeclId::new(i);
                count += 1;
                let (old, new) = (s.written_structural(d), s.written_by_table(d));
                if old == new {
                    continue;
                }
                let k = s.kind(d);
                let decl = k != DeclKind::TranslationUnit
                    && k != DeclKind::ParmVar
                    && s.as_written(d) == d
                    && super::super::entities::value_kind(k);
                let row = written::row(p.unit(), d).map(|r| format!("{r:?}"));
                let e = tally.entry((format!("{k:?} row={row:?}"), old, new, decl)).or_default();
                e.0 += 1;
                if e.1.len() < 4 {
                    e.1.push(format!(
                        "{label}: {} at {}",
                        slip_digest::display_name(p.ast, d),
                        s.describe_loc(p.ast.decls.get(d).loc())
                    ));
                }
            }
            count
        })
        .unwrap_or(0)
    }

    #[test]
    #[ignore = "WP-085's comparison, run by hand: cargo test -p slip_script --lib compare_written -- --ignored --nocapture"]
    fn written_structural_against_table_over_digest_pair_tests() {
        let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../slip_digest/tests");
        let mut support: Vec<(String, String)> = Vec::new();
        for e in std::fs::read_dir(tests.join("support")).into_iter().flatten().flatten() {
            if let (Some(n), Ok(t)) =
                (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
            {
                support.push((n, t));
            }
        }
        support.sort();
        let dirs: Vec<PathBuf> = ["differ", "equal", "demand", "indeterminate"]
            .iter()
            .flat_map(|r| pair_dirs(&tests.join(r)))
            .collect();
        assert!(dirs.len() > 1000, "found only {} pair-test directories", dirs.len());
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4);
        let next = std::sync::atomic::AtomicUsize::new(0);
        let parts: Vec<(Tally, u64)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let (mut t, mut n) = (Tally::new(), 0);
                        loop {
                            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(dir) = dirs.get(k) else { break };
                            let label =
                                dir.strip_prefix(&tests).unwrap_or(dir).display().to_string();
                            for (i, side) in sides(dir, &support).iter().enumerate() {
                                n += compare(side, &format!("{label} side {i}"), &mut t);
                            }
                        }
                        (t, n)
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        let (mut all, mut decls) = (Tally::new(), 0);
        for (t, n) in parts {
            decls += n;
            for (k, (c, ex)) in t {
                let e = all.entry(k).or_default();
                e.0 += c;
                e.1.extend(ex);
                e.1.truncate(4);
            }
        }
        eprintln!("{} directories, {decls} declarations", dirs.len());
        for ((kind, old, new, decl), (c, ex)) in &all {
            eprintln!("{kind} structural={old} table={new} declaration-kind={decl}: {c}");
            for e in ex {
                eprintln!("    {e}");
            }
        }
    }
}
