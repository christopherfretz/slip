//! The `negatives` section, id 27 of a map shard (`spec/model.md` §8.6;
//! work package WP-117): the places the map parse looked for a file and
//! found none, sorted, each path front-coded against the one before.

use crate::Phase;
use crate::json::Value;
use crate::wire::{Decoder, Encoder};

use super::At;
use super::common::{JResult, Result, json_bytes, malformed, read_bytes, read_list};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 27;

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Entry(usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        match self {
            Loc::Section => "sections.negatives".to_owned(),
            Loc::Entry(i) => format!("sections.negatives[{i}]"),
        }
    }
}

/// The first violation (§8.6), in this order: the shard's phase is `map`;
/// it has `inputs` (so a parse happened, and its status is not `failed`);
/// then for each path in turn: absolute, then strictly above the one
/// before.
pub(super) fn violation(
    paths: &[Vec<u8>],
    phase: Phase,
    inputs: bool,
) -> Option<(Loc, &'static str)> {
    if phase != Phase::Map {
        return Some((Loc::Section, "a negatives section in a shard whose phase is not map"));
    }
    if !inputs {
        return Some((Loc::Section, "a negatives section in a shard without inputs"));
    }
    for (i, p) in paths.iter().enumerate() {
        if !p.starts_with(b"/") {
            return Some((Loc::Entry(i), "a negative input is not an absolute path"));
        }
        if i > 0 && paths[i - 1] >= *p {
            return Some((Loc::Entry(i), "negatives are not strictly ascending"));
        }
    }
    None
}

/// The length of the longest common prefix of `a` and `b`.
fn common(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

// ---------------------------------------------------------------------------
// Binary

/// `list<Negative>`, `Negative := shared: uv, rest: bytes`: each path is
/// the first `shared` bytes of the one before (the empty path before the
/// first) then `rest`, `shared` the length of their longest common prefix.
pub(super) fn encode(e: &mut Encoder, paths: &[Vec<u8>]) {
    e.count(paths.len());
    let mut prev: &[u8] = &[];
    for p in paths {
        let shared = common(prev, p);
        e.uv(shared as u64).bytes(&p[shared..]);
        prev = p;
    }
}

/// File offsets of what an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    entries: Vec<usize>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Section => self.section,
            Loc::Entry(i) => self.entries[i],
        }
    }
}

/// The paths, and where each begins. The encoding's own rules are checked
/// here, each at the `shared` field: it is at most the length of the path
/// before, and it is the longest common prefix (one list, one encoding).
pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Vec<Vec<u8>>, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let n = d.count()?;
    let mut paths: Vec<Vec<u8>> = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        let at = d.offset();
        o.entries.push(at);
        let shared = d.uv()?;
        let rest = d.bytes()?;
        let prev: &[u8] = paths.last().map_or(&[], Vec::as_slice);
        let shared = match usize::try_from(shared) {
            Ok(s) if s <= prev.len() => s,
            _ => return Err(malformed(at, "a shared prefix longer than the path before")),
        };
        if shared < prev.len() && rest.first() == Some(&prev[shared]) {
            return Err(malformed(at, "a shared prefix that is not the longest"));
        }
        let mut path = Vec::with_capacity(shared + rest.len());
        path.extend_from_slice(&prev[..shared]);
        path.extend_from_slice(rest);
        paths.push(path);
    }
    Ok((paths, o))
}

// ---------------------------------------------------------------------------
// JSON

/// The paths, whole, in order (the front-coding is the binary form's).
pub(super) fn render(paths: &[Vec<u8>]) -> Value {
    Value::Array(paths.iter().map(|p| json_bytes(p)).collect())
}

pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Vec<Vec<u8>>> {
    read_list(v, at, read_bytes)
}
