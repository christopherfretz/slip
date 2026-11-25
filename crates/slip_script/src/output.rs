//! The run output (`spec/api.md` §7.6) and its canonical form.
//!
//! The host records requests as they come into a raw [`RunOutput`] (edits
//! in request order, groups as lists of raw edit indices) and
//! [`RunOutput::canonical`] turns it into the form §7.6 fixes: nothing in
//! it depends on the order of the requests or on a number the host
//! assigned. The encoding in a shard is `spec/edits.md`'s, not this
//! module's.

use std::fmt;
use std::sync::Arc;

use slip_model::keys::{Name, Parent, Rule};
use slip_model::{Counter, Digest, FileRef};

/// A run of bytes of a file, named by the file's identity (§7.6
/// `(file, lo, hi)`). Ordered by file, then `lo`, then `hi`.
///
/// The identity is shared: the host makes one `FileRef` per file of the
/// run and every place in that file holds it, so a run holds each path
/// once however many places name it (§7.7). Equality and order are the
/// identity's, not the pointer's.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Place {
    pub file: Arc<FileRef>,
    pub lo: u32,
    pub hi: u32,
}

/// One piece of a replacement (§7.1, §7.6 `Piece`). The derived order
/// (literal, then copy, then spelled, then identifier; then by content)
/// is the order of edits with the same range.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Piece {
    Literal(Vec<u8>),
    Copy(Place),
    /// The entity meant (its key) and the text chosen for it (§7.2).
    Spelled {
        key: Digest,
        text: Vec<u8>,
    },
    /// One identifier given as data (§7.1 rule 7); not empty. `key` is
    /// present when the piece is the whole replacement of the name range
    /// of references whose referents' origin is declared renamed to `text`
    /// (§7.2 rule 6, §7.5 rule 3): then it is that entity's key, checked
    /// as a spelled piece's is (§7.2 rule 5).
    Identifier {
        text: Vec<u8>,
        key: Option<Digest>,
    },
}

/// The tiers at which the requests that are one edit were made (§7.6
/// `Edit.tiers`, §5.4): a set of 0 and 1 that is never empty. One range
/// can be reached at both tiers in one run (a header included twice,
/// with a name that is a macro in one inclusion and a function in the
/// other), so it is a set and not a number. Bit `t` of the private byte
/// is tier `t`, as in a claim's `tiers` (`spec/edits.md` §4.2).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tiers(u8);

impl Tiers {
    /// Tier 0 alone.
    pub const ZERO: Tiers = Tiers(0b01);
    /// Tier 1 alone.
    pub const ONE: Tiers = Tiers(0b10);
    /// Tiers 0 and 1.
    pub const BOTH: Tiers = Tiers(0b11);

    /// The set of the tiers `t` with `reached[t]`; `None` when it would
    /// be empty.
    pub const fn from_reached(reached: [bool; 2]) -> Option<Tiers> {
        match reached {
            [false, false] => None,
            [true, false] => Some(Tiers::ZERO),
            [false, true] => Some(Tiers::ONE),
            [true, true] => Some(Tiers::BOTH),
        }
    }

    /// Bit `t` set for tier `t`; never 0, never above 3.
    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, tier: u8) -> bool {
        tier < 2 && self.0 & (1 << tier) != 0
    }

    #[must_use]
    pub const fn union(self, other: Tiers) -> Tiers {
        Tiers(self.0 | other.0)
    }

    /// The tiers in ascending order.
    pub fn iter(self) -> impl Iterator<Item = u8> {
        (0..2u8).filter(move |&t| self.contains(t))
    }
}

/// `0`, `1` or `0,1`, the form `RunOutput::render` writes.
impl fmt::Display for Tiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, t) in self.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{t}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Tiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tiers({self})")
    }
}

/// One requested replacement (§7.6 `Edit`). Its identity is its place and
/// its pieces; the notes and the tiers are not part of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Edit {
    /// `lo < hi`.
    pub place: Place,
    pub pieces: Vec<Piece>,
    /// The tiers of the requests that are this edit (§7.6, §5.4).
    pub tiers: Tiers,
    /// Sorted, distinct (§7.3 rule 5).
    pub notes: Vec<Vec<u8>>,
}

/// `slip::severity` (§7.4). Open in the header; the host knows these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Severity {
    Note = 0,
    Review = 1,
    Error = 2,
}

impl Severity {
    /// The severity of a wire value; `None` for one the host does not know.
    pub fn from_wire(v: u64) -> Option<Severity> {
        match v {
            0 => Some(Severity::Note),
            1 => Some(Severity::Review),
            2 => Some(Severity::Error),
            _ => None,
        }
    }
}

/// One report (§7.4, §7.6 `Report`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Report {
    pub place: Option<Place>,
    pub severity: Severity,
    pub message: Vec<u8>,
    /// §7.4 rule 3: `true` when the place is not the value's own: the
    /// report is attached to the nearest place the host can name, or, with
    /// no place, the host could name none (`spec/landed/api/WP-081.md`,
    /// `WP-088.md`). `false` with no place: nothing to attach to.
    pub attached: bool,
}

/// One declared change (§7.5, §7.6 `Changed`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Changed {
    pub key: Digest,
    pub place: Place,
    pub reason: Vec<u8>,
}

/// The output of a completed run (§7.6 `RunOutput`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOutput {
    pub transform: Digest,
    pub edits: Vec<Edit>,
    /// Canonical: each a sorted list of two or more indices into `edits`,
    /// no edit in two groups, the list sorted. Raw (while the host
    /// records): one list per group made, of raw edit indices, in any
    /// order and with any repetition.
    pub groups: Vec<Vec<usize>>,
    pub reports: Vec<Report>,
    pub changed: Vec<Changed>,
    /// `spec/verify.md` §2.1; sorted as a rule table.
    pub renamed: Vec<Rule>,
    pub counters: Vec<Counter>,
}

impl RunOutput {
    /// An output with nothing in it.
    pub fn empty(transform: Digest) -> RunOutput {
        RunOutput {
            transform,
            edits: Vec::new(),
            groups: Vec::new(),
            reports: Vec::new(),
            changed: Vec::new(),
            renamed: Vec::new(),
            counters: Vec::new(),
        }
    }

    /// The canonical form of §7.6: pieces normalised (adjacent literals
    /// joined, empty literals dropped, §7.1 rule 6); edits sorted by
    /// (place, pieces) and identical ones joined (notes and tiers
    /// unioned, §7.6 rule 1); groups rewritten to the joined indices, groups
    /// that share a member merged, groups of fewer than two members
    /// dropped; reports, declared changes and rules sorted and
    /// deduplicated; counters sorted by name.
    ///
    /// `self.groups` is read as raw: indices into `self.edits` as given.
    /// The function is total: an index out of range names no edit and is
    /// ignored.
    #[must_use]
    pub fn canonical(self) -> RunOutput {
        let RunOutput { transform, edits, groups, mut reports, mut changed, renamed, mut counters } =
            self;

        // Edits: normalise, sort, join.
        let raw_len = edits.len();
        let mut order: Vec<(usize, Edit)> = edits
            .into_iter()
            .enumerate()
            .map(|(i, mut e)| {
                e.pieces = normalise(std::mem::take(&mut e.pieces));
                (i, e)
            })
            .collect();
        order.sort_by(|a, b| (&a.1.place, &a.1.pieces).cmp(&(&b.1.place, &b.1.pieces)));
        let mut joined: Vec<Edit> = Vec::with_capacity(order.len());
        let mut new_index = vec![0usize; raw_len];
        for (raw, edit) in order {
            match joined.last_mut() {
                Some(last) if last.place == edit.place && last.pieces == edit.pieces => {
                    last.notes.extend(edit.notes);
                    last.tiers = last.tiers.union(edit.tiers);
                }
                _ => joined.push(edit),
            }
            new_index[raw] = joined.len() - 1;
        }
        for e in &mut joined {
            e.notes.sort();
            e.notes.dedup();
        }

        // Groups: union-find over the joined edits.
        let mut parent: Vec<usize> = (0..joined.len()).collect();
        let mut grouped = vec![false; joined.len()];
        for group in &groups {
            let mut first: Option<usize> = None;
            for &raw in group {
                let Some(&e) = new_index.get(raw) else { continue };
                grouped[e] = true;
                match first {
                    None => first = Some(e),
                    Some(f) => union(&mut parent, f, e),
                }
            }
        }
        let mut by_root: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (e, _) in grouped.iter().enumerate().filter(|(_, g)| **g) {
            let r = find(&mut parent, e);
            by_root.entry(r).or_default().push(e);
        }
        let mut groups: Vec<Vec<usize>> = by_root.into_values().filter(|g| g.len() >= 2).collect();
        groups.sort();

        reports.sort();
        reports.dedup();
        changed.sort();
        changed.dedup();

        let mut renamed: Vec<(Vec<u8>, Rule)> = renamed
            .into_iter()
            .map(|r| {
                let mut enc = slip_model::wire::Encoder::new();
                r.encode(&mut enc);
                (enc.into_bytes(), r)
            })
            .collect();
        // The encoding of a rule begins with the encoding of its match,
        // which is self-delimiting, so ordering by the whole encoding is
        // ordering by the match first: the rule table's order.
        renamed.sort_by(|a, b| a.0.cmp(&b.0));
        renamed.dedup_by(|a, b| a.0 == b.0);
        let renamed = renamed.into_iter().map(|(_, r)| r).collect();

        counters.sort();
        counters.dedup();

        RunOutput { transform, edits: joined, groups, reports, changed, renamed, counters }
    }

    /// Whether the output is already in canonical form.
    pub fn is_canonical(&self) -> bool {
        self.clone().canonical() == *self
    }

    /// One line per item, places as `path:lo-hi`; the form that `slip
    /// dump` will print. It is [`RunOutput::render_styled`] with
    /// [`Style::default`].
    ///
    /// ```text
    /// transform <digest>
    /// edit <i> <place> tiers <0 | 1 | 0,1> = <piece> ... [note "<text>"] ...
    /// group <i> <i> ...
    /// report <place | -> <note | review | error> "<message>" [attached]
    /// changed <key> <place> "<reason>"
    /// renamed <kind> <parent> <name> [disc <digest>] -> <parent> <name>
    /// counter <name> <value>
    /// ```
    ///
    /// A piece is `"<bytes>"`, `copy(<place>)`, `spell(<key> "<text>")`,
    /// `identifier("<name>")` or, with a key, `identifier(<key> "<name>")`.
    /// A name is its identifier quoted, or the form's name of
    /// `spec/digest-kinds.md` §3.2 with its operand in parentheses
    /// (`operator(3)`, `conversion(<digest>)`, `literal_operator("_km")`,
    /// `unnamed_by(<name>)`, `constructor`).
    /// Text is quoted with `\\`, `\"`, `\n`, `\t` and `\xNN` for any
    /// other byte outside printable ASCII; a path is written the same way
    /// without the quotes. A digest is its 64 hexadecimal digits. Items come
    /// in the order of the output, so the canonical output renders
    /// canonically.
    pub fn render(&self, f: &mut dyn fmt::Write) -> fmt::Result {
        self.render_styled(f, &Style::default())
    }

    /// [`RunOutput::render`] with the transform id, the positions of places
    /// and the keys written as `style` says (the script tests of WP-090,
    /// `spec/landed/api/WP-090.md`). A key `style` names is written as its
    /// name, quoted; any other as its digest. A key is the `<key>` of a
    /// spelled piece and of `changed`, and a parent that is a unit.
    pub fn render_styled(&self, f: &mut dyn fmt::Write, style: &Style<'_>) -> fmt::Result {
        let s = style;
        match s.transform {
            Some(t) => writeln!(f, "transform {t}")?,
            None => writeln!(f, "transform {}", self.transform)?,
        }
        for (i, e) in self.edits.iter().enumerate() {
            write!(f, "edit {i} {} tiers {} =", Rendered::Place(&e.place, s), e.tiers)?;
            for p in &e.pieces {
                match p {
                    Piece::Literal(b) => write!(f, " {}", Rendered::Quoted(b))?,
                    Piece::Copy(at) => write!(f, " copy({})", Rendered::Place(at, s))?,
                    Piece::Spelled { key, text } => {
                        write!(f, " spell({} {})", Rendered::Key(key, s), Rendered::Quoted(text))?;
                    }
                    Piece::Identifier { text, key: None } => {
                        write!(f, " identifier({})", Rendered::Quoted(text))?;
                    }
                    Piece::Identifier { text, key: Some(key) } => {
                        write!(
                            f,
                            " identifier({} {})",
                            Rendered::Key(key, s),
                            Rendered::Quoted(text)
                        )?;
                    }
                }
            }
            for n in &e.notes {
                write!(f, " note {}", Rendered::Quoted(n))?;
            }
            writeln!(f)?;
        }
        for g in &self.groups {
            write!(f, "group")?;
            for i in g {
                write!(f, " {i}")?;
            }
            writeln!(f)?;
        }
        for r in &self.reports {
            match &r.place {
                Some(p) => write!(f, "report {}", Rendered::Place(p, s))?,
                None => write!(f, "report -")?,
            }
            let severity = match r.severity {
                Severity::Note => "note",
                Severity::Review => "review",
                Severity::Error => "error",
            };
            write!(f, " {severity} {}", Rendered::Quoted(&r.message))?;
            writeln!(f, "{}", if r.attached { " attached" } else { "" })?;
        }
        for c in &self.changed {
            writeln!(
                f,
                "changed {} {} {}",
                Rendered::Key(&c.key, s),
                Rendered::Place(&c.place, s),
                Rendered::Quoted(&c.reason)
            )?;
        }
        for r in &self.renamed {
            write!(
                f,
                "renamed {} {} {}",
                r.kind.name(),
                Rendered::Parent(&r.parent, s),
                Rendered::Name(&r.name)
            )?;
            if let Some(d) = &r.disc {
                write!(f, " disc {d}")?;
            }
            writeln!(
                f,
                " -> {} {}",
                Rendered::Parent(&r.to_parent, s),
                Rendered::Name(&r.to_name)
            )?;
        }
        for c in &self.counters {
            writeln!(f, "counter {} {}", c.name, c.value)?;
        }
        Ok(())
    }
}

/// What [`Style::position`] holds: what follows a place's path.
pub type PositionFn<'a> = dyn Fn(&Place) -> String + 'a;

/// What [`Style::key_name`] holds: the name of a key, if it has one.
pub type KeyNameFn<'a> = dyn Fn(&Digest) -> Option<Vec<u8>> + 'a;

/// How [`RunOutput::render_styled`] writes what is not the same on every
/// machine or is not for people as a number. `Default` is
/// [`RunOutput::render`]'s.
#[derive(Clone, Copy, Default)]
pub struct Style<'a> {
    /// The text of the `transform` line; `None`: the id's digest.
    pub transform: Option<&'a str>,
    /// What follows a place's path; `None`: `:<lo>-<hi>`.
    pub position: Option<&'a PositionFn<'a>>,
    /// The name of a key, when it has one; `None`, or a key it gives no
    /// name: the key's digest.
    pub key_name: Option<&'a KeyNameFn<'a>>,
}

impl fmt::Debug for Style<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Style")
            .field("transform", &self.transform)
            .field("position", &self.position.is_some())
            .field("key_name", &self.key_name.is_some())
            .finish()
    }
}

/// The pieces of a rendered line.
enum Rendered<'a> {
    Quoted(&'a [u8]),
    Place(&'a Place, &'a Style<'a>),
    Key(&'a Digest, &'a Style<'a>),
    Parent(&'a Parent, &'a Style<'a>),
    Name(&'a Name),
}

fn escaped(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for &b in bytes {
        match b {
            b'\\' => f.write_str("\\\\")?,
            b'"' => f.write_str("\\\"")?,
            b'\n' => f.write_str("\\n")?,
            b'\t' => f.write_str("\\t")?,
            0x20..=0x7e => write!(f, "{}", b as char)?,
            _ => write!(f, "\\x{b:02x}")?,
        }
    }
    Ok(())
}

impl fmt::Display for Rendered<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rendered::Quoted(b) => {
                f.write_str("\"")?;
                escaped(f, b)?;
                f.write_str("\"")
            }
            Rendered::Place(p, s) => {
                escaped(f, &p.file.path)?;
                match s.position {
                    Some(position) => f.write_str(&position(p)),
                    None => write!(f, ":{}-{}", p.lo, p.hi),
                }
            }
            Rendered::Key(k, s) => match s.key_name.and_then(|name| name(k)) {
                Some(name) => write!(f, "{}", Rendered::Quoted(&name)),
                None => write!(f, "{k}"),
            },
            Rendered::Parent(p, s) => match p {
                Parent::Root => f.write_str("root"),
                Parent::CLinkage => f.write_str("c_linkage"),
                Parent::Builtin => f.write_str("builtin"),
                Parent::Unit(d) => write!(f, "{}", Rendered::Key(d, s)),
            },
            Rendered::Name(n) => match n {
                Name::Identifier(b) => write!(f, "{}", Rendered::Quoted(b)),
                Name::Operator(k) => write!(f, "operator({k})"),
                Name::Constructor => f.write_str("constructor"),
                Name::Destructor => f.write_str("destructor"),
                Name::Conversion(d) => write!(f, "conversion({d})"),
                Name::LiteralOperator(b) => write!(f, "literal_operator({})", Rendered::Quoted(b)),
                Name::DeductionGuide => f.write_str("deduction_guide"),
                Name::Anonymous => f.write_str("anonymous"),
                Name::Content => f.write_str("content"),
                Name::Local(k) => write!(f, "local({k})"),
                Name::UnnamedBy(inner) => write!(f, "unnamed_by({})", Rendered::Name(inner)),
                Name::UnnamedOrdinal(k) => write!(f, "unnamed_ordinal({k})"),
                Name::Property(k) => write!(f, "property({k})"),
                Name::Order => f.write_str("order"),
                Name::Parameter(k) => write!(f, "parameter({k})"),
                Name::Operands => f.write_str("operands"),
            },
        }
    }
}

/// §7.1 rule 6: adjacent literals are one piece; an empty literal is no piece.
fn normalise(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for p in pieces {
        match p {
            Piece::Literal(bytes) if bytes.is_empty() => {}
            Piece::Literal(bytes) => {
                if let Some(Piece::Literal(prev)) = out.last_mut() {
                    prev.extend_from_slice(&bytes);
                } else {
                    out.push(Piece::Literal(bytes));
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[hi] = lo;
    }
}
