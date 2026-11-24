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
    /// The origin (§5.5, `slip_kiln::written::origin`, as an entity) of
    /// each declaration that is not its own.
    origins: HashMap<u32, DeclId>,
    /// Origin (an entity's number) → its references, in table order.
    by_origin: OnceLock<HashMap<u32, Vec<u32>>>,
    /// `references_to` answers already sorted into unit order.
    sorted: Mutex<HashMap<u32, Arc<[Reference]>>>,
    /// Extent → the references that have it.
    by_extent: OnceLock<HashMap<Range, Vec<Reference>>>,
    /// The entries (indexes in `KilnSubject::refs`) joined into a
    /// reference after its first ([`Ref::entry`]), by reference (WP-095:
    /// the holder of a reference is that of all its entries).
    more_entries: HashMap<u32, Vec<u32>>,
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
            + self.origins.capacity() * (8 + 8)
            + self.more_entries.values().map(|v| v.capacity() * 4 + 40).sum::<usize>()
            + self.unresolved.keys().map(|k| k.len() + 40).sum::<usize>()
    }
}

/// One reference.
struct Ref {
    /// The entity the name denotes (Kiln's referent, as the subject
    /// numbers entities).
    referent: DeclId,
    /// The referent's origin (§5.5).
    origin: DeclId,
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
    /// The first entry joined into this reference: its index in
    /// `KilnSubject::refs` (the same list, in the same order, as
    /// `written::written_refs` gives [`Names::add_references`]).
    entry: u32,
}

/// The name range of a declaration, or the nearest place.
struct Place {
    name: Option<Given>,
    nearest: Option<Range>,
}

/// What makes two entries one reference (module documentation): not the
/// role nor the referent, so that a name Kiln records twice (a template's
/// name as a template and as the type its arguments make: `S` and
/// `S<int>`) is one reference to their origin.
#[derive(PartialEq, Eq, Hash)]
struct Key {
    origin: u32,
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
            origins: origins(s, unit),
            ..Names::default()
        };
        names.add_references(s, unit);
        names
    }

    /// The origin of `d` (an entity).
    pub(super) fn origin(&self, d: DeclId) -> DeclId {
        self.origins.get(&d.index()).copied().unwrap_or(d)
    }

    /// §5.4, §7.3 rule 2: the tiers of the name range of each live
    /// declaration (`live`), once the subject can tell which are.
    pub(super) fn add_declaration_tiers(&mut self, live: impl Fn(u32) -> bool) {
        for (&d, p) in &self.decls {
            if let Some(g) = p.name.filter(|_| live(d)) {
                self.tiers.entry(g.0).or_insert([false, false])[g.1 as usize] = true;
            }
        }
    }

    /// The references of §6.5 rule 3, one per written name (module
    /// documentation).
    fn add_references(&mut self, s: &KilnSubject<'_>, unit: &Unit<'_>) {
        let sm = unit.sm();
        let refs = written::written_refs(unit);
        let declarative = Declarative::build(unit, &refs);
        let mut keys: HashMap<Key, u32> = HashMap::new();
        for (j, r) in refs.into_iter().enumerate() {
            let j = j as u32;
            if !returned_role(r.role) || declarative.holds(&r) {
                continue;
            }
            let referent = s.entity(r.referent);
            if !s.is_entity(referent) {
                continue;
            }
            let mut x = s.make_ref(unit, &r, referent);
            x.entry = j;
            x.origin = origin_of(s, unit, referent);
            let key = Key {
                origin: x.origin.index(),
                spelled: sm.offset_of(sm.spelling_loc(r.name.lo)),
                invocation: s.invocation(r.name.lo),
                name: x.name.map(|g| g.0),
                extent: x.extent.map(|g| g.0),
                direct: x.direct,
            };
            // A later entry of one written name keeps the first entry's
            // first token: Kiln records the entries of one written name in
            // the order of their preprocessing, which is unit order.
            let at = if let Some(&i) = keys.get(&key) {
                self.more_entries.entry(i).or_default().push(j);
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

/// §5.5: `slip_kiln::written::origin` of `d`, as an entity of the subject
/// (`d` itself when that is none).
fn origin_of(s: &KilnSubject<'_>, unit: &Unit<'_>, d: DeclId) -> DeclId {
    let o = s.entity(written::origin(unit, d));
    if s.is_entity(o) { o } else { d }
}

/// The origin of every entity that is not its own.
fn origins(s: &KilnSubject<'_>, unit: &Unit<'_>) -> HashMap<u32, DeclId> {
    let mut out = HashMap::new();
    for n in 1..=u32::try_from(unit.ast().decls.len()).unwrap_or(u32::MAX) {
        let Some(d) = s.decl(n) else { break };
        if s.entity(d) == d && s.is_entity(d) {
            let o = origin_of(s, unit, d);
            if o != d {
                out.insert(n, o);
            }
        }
    }
    out
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
        let loc = unit.ast().decls.get(n.decl).loc();
        if loc != n.name.lo {
            // Kiln K3: a later declaration overwrote the row, whose name is
            // then another declaration's token. Refused: no name range,
            // and the nearest place is the declaration's own token.
            out.insert(n.decl.index(), Place { name: None, nearest: s.own_token(unit, loc) });
            continue;
        }
        let ident = s.identifier_of(n.decl);
        let name = s.given(name_token(unit, n.name, &ident));
        let nearest = match name {
            Some(_) => None,
            None => s
                .given(ranges::file_range_of_span(unit.sm(), n.name))
                .map(|g| g.0)
                .or_else(|| s.invocation(n.name.lo)),
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
        if let Some(t) = identifier_at(unit, n.name.lo) {
            *out.entry(t).or_insert(0) += 1;
        }
    }
    out
}

/// The identifier of the token spelled where `at` is, as Kiln's lexer
/// reads it (line splices removed, universal-character-names decoded:
/// `caf\u00e9` is `café`), so that names are compared as identifiers, not
/// as bytes. `None` when that token is no identifier.
fn identifier_at(unit: &Unit<'_>, at: Loc) -> Option<Vec<u8>> {
    use slip_kiln::kiln::lex::{LexMode, LexOptions, Lexer, TokKind};
    let sm = unit.sm();
    let spelled = sm.spelling_loc(at);
    let (file, pos) = sm.offset_of(spelled)?;
    let len = u32::try_from(sm.text(file).len()).ok()?;
    if pos >= len {
        return None;
    }
    let base = Loc::from_raw(spelled.to_raw() - pos);
    let symbols = unit.ast().symbols();
    let mut lx = Lexer::new_in_range(sm, symbols, file, pos..len, base, LexOptions::default());
    lx.set_mode(LexMode::Normal);
    match lx.next().kind {
        TokKind::Identifier(s) => Some(symbols.as_str(s).as_bytes().to_vec()),
        _ => None,
    }
}

/// The declarative contexts of §6.5 rule 4 whose names Kiln records in a
/// role of rule 3: the qualifier of a declarator-id, of a class head, of
/// an explicit specialisation's name, of a using-declaration, of a
/// using-enum-declaration's enumeration (and that enumeration's own name),
/// of a using-directive's or namespace alias's namespace. Each is a
/// qualifier chain, and every component of a chain has the chain's first
/// token as its `first` (`slip_kiln::written`), so a context is that first
/// token: the row's `qual` (a using-enum-declaration's `name` when it has
/// none). Since Kiln WP-1932 (stage 3) the names of rule 4 have entries
/// of rule 4's roles, which are not returned; their qualifiers are still
/// of rule 3's roles, so a row gives each chain's first token, and an
/// `Elaborated` entry gives a friend's elaborated specifier's. A name inside
/// the chain that is not a component of it (a template argument,
/// `n::W<n::A>::k`'s `n::A`) has a `first` of its own, and is returned.
/// A namespace alias's target has no row `qual` and no entry of its own
/// (Kiln records only its qualifiers), so a using-directive and a
/// namespace alias are taken whole: every name in them, as raw locations
/// of one entry (a namespace name has no template arguments).
struct Declarative {
    /// The raw locations of the first tokens of the chains.
    starts: HashSet<u32>,
    /// The using-directives and namespace aliases, as raw `[lo, hi)`,
    /// sorted and joined.
    whole: Vec<(u32, u32)>,
}

impl Declarative {
    fn build(unit: &Unit<'_>, refs: &[WrittenRef]) -> Declarative {
        let ast = unit.ast();
        let sm = unit.sm();
        let mut starts = HashSet::new();
        // An elaborated reference (a friend's `friend struct a::S;`, a
        // declaration `class X;`) is rule 4's, and so is its qualifier
        // chain: Kiln records it since its WP-1932 (role `Elaborated`),
        // and a friend naming a class declared before it no longer
        // overwrites the class's row (the K3 path below).
        starts.extend(refs.iter().filter(|r| r.role == Role::Elaborated).map(|r| r.first.to_raw()));
        let mut whole = Vec::new();
        // A friend's elaborated specifier (`friend struct a::S;`) records
        // no entry, and overwrites the class's row (Kiln K3): its
        // qualifier is reached through that row, whose name is then inside
        // the friend.
        let friends: Vec<(u32, u32)> = ast
            .decl_extents
            .iter()
            .filter(|&(d, row)| {
                ast.decls.get(d).kind() == DeclKind::Friend
                    && sm.same_entry_distance(row.full.lo, row.full.hi).is_some()
            })
            .map(|(_, row)| (row.full.lo.to_raw(), row.full.hi.to_raw()))
            .collect();
        let in_friend = |at: Loc| {
            let at = at.to_raw();
            friends.iter().any(|&(lo, hi)| lo <= at && at < hi)
        };
        for (d, row) in ast.decl_extents.iter() {
            let kind = ast.decls.get(d).kind();
            if matches!(kind, DeclKind::UsingDirective | DeclKind::NamespaceAlias)
                && sm.same_entry_distance(row.full.lo, row.full.hi).is_some()
            {
                whole.push((row.full.lo.to_raw(), row.full.hi.to_raw()));
            }
            // A row whose name is not at its declaration's location is
            // another declaration's (Kiln K3): its qualifier is a
            // declarator's only when that declaration is a friend.
            let own = row.name.lo == ast.decls.get(d).loc() || in_friend(row.name.lo);
            if row.qual.is_valid() && own {
                starts.insert(row.qual.to_raw());
            } else if kind == DeclKind::UsingEnum && row.name.lo.is_valid() {
                starts.insert(row.name.lo.to_raw());
            }
        }
        whole.sort_unstable();
        let mut joined: Vec<(u32, u32)> = Vec::with_capacity(whole.len());
        for (lo, hi) in whole {
            match joined.last_mut() {
                Some(last) if lo <= last.1 => last.1 = last.1.max(hi),
                _ => joined.push((lo, hi)),
            }
        }
        Declarative { starts, whole: joined }
    }

    /// Whether the name of `r` is a component of a declarative chain, or
    /// in a using-directive or namespace alias.
    fn holds(&self, r: &WrittenRef) -> bool {
        let at = r.name.lo.to_raw();
        let k = self.whole.partition_point(|&(lo, _)| lo <= at);
        self.starts.contains(&r.first.to_raw()) || (k > 0 && at < self.whole[k - 1].1)
    }
}

/// §6.6 rule 1, the one rule for a name token, for references and
/// declarations alike: the range of the span `name` when it is the one
/// identifier token `ident`. It is `slip_kiln::ranges::token_range_or_why`
/// (the range of the one token at `name.lo`, the rule
/// `slip_kiln::sites::Sites::name_token` uses), kept when the token is the
/// one spelled where the name's first character is (not an invocation
/// holding it: `ID(foo)`, `#define c c`) and its bytes are `ident` (not
/// written with a line splice or a universal-character-name; never an
/// operator, constructor, destructor or conversion function, whose
/// identifier is empty and whose range is not). `tests/subject_references.rs`
/// checks it against `Sites::name_token` over the pair corpus.
pub fn name_token(unit: &Unit<'_>, name: Span, ident: &[u8]) -> Option<FileRange> {
    let sm = unit.sm();
    let fr = ranges::token_range_or_why(unit, name).ok()?;
    let at = sm.offset_of(sm.spelling_loc(name.lo)) == Some((fr.file, fr.lo));
    (at && sm.text(fr.file).as_bytes().get(fr.lo as usize..fr.hi as usize) == Some(ident))
        .then_some(fr)
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
        // A qualifier the table does not link (`decltype(s)::m`, a
        // pack-index-specifier) ends the chain, so `first` is the component
        // after its `::`: the start of the written name is not known, and
        // an extent from `first` would be too short (§6.6 rule 4).
        if let Some((_, "::")) = token_before(unit, r.first) {
            return None;
        }
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

    /// The nearest place of a declaration whose row is another's (Kiln
    /// K3): the token at its own location, or the invocation that produced
    /// it.
    fn own_token(&self, unit: &Unit<'_>, loc: Loc) -> Option<Range> {
        self.given(ranges::file_range(unit, loc, loc)).map(|g| g.0).or_else(|| self.invocation(loc))
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
        let mut name = self.given(name_token(unit, r.name, &ident));
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
            origin: referent,
            first: r.first,
            end: r.name.hi,
            name,
            extent,
            nearest,
            direct: !r.via_using,
            entry: 0,
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

    /// The entries of reference `r` (indexes in `self.refs`), the first
    /// first: one written name may be several entries of Kiln's table (a
    /// header entered twice, a macro argument used twice).
    pub(super) fn reference_entries(&self, r: Reference) -> Vec<u32> {
        let Some(x) = self.reference(r) else { return Vec::new() };
        let mut out = vec![x.entry];
        out.extend(self.names().more_entries.get(&r.0).into_iter().flatten());
        out
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
                map.entry(x.origin.index()).or_default().push(i as u32);
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
            // Unit order (§5.3 rule 2), as `references_to`.
            for v in map.values_mut() {
                v.sort_by(|a, b| {
                    let (x, y) = (&names.refs[a.0 as usize], &names.refs[b.0 as usize]);
                    super::unit_order(self.sm, x.first, y.first)
                        .then_with(|| super::unit_order(self.sm, y.end, x.end))
                        .then(a.0.cmp(&b.0))
                });
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
        // A row whose name does not start at the declaration's own
        // location is another declaration's (Kiln K3): not looked at.
        let loc = self.ast.decls.get(d).loc();
        let named = self.ast.decl_extent(d).map(|x| x.name).filter(|n| n.lo == loc);
        let nearest = named
            .and_then(|n| self.given(ranges::file_range_of_span(self.sm, n)).map(|g| g.0))
            .or_else(|| self.invocation(loc));
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
