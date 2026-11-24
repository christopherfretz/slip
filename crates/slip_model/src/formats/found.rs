//! The `found` section, id 31 of a map shard (`spec/model.md` §8.8; work
//! package WP-135): each place where the map parse found a file, with the
//! input it reached (the paths front-coded as `negatives` codes them), and
//! the classes of `inputs` that were one file at the map phase.

use std::collections::HashSet;

use crate::json::Value;
use crate::wire::{Decoder, Encoder};
use crate::{Found, FoundPlace, Phase};

use super::At;
use super::common::{
    JResult, Result, json_bytes, json_uv, malformed, object, read_bytes, read_list, read_record,
    read_uv,
};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 31;

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Place(usize),
    Input(usize),
    Class(usize),
    Member(usize, usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        match self {
            Loc::Section => "sections.found".to_owned(),
            Loc::Place(i) => format!("sections.found.places[{i}]"),
            Loc::Input(i) => format!("sections.found.places[{i}].input"),
            Loc::Class(j) => format!("sections.found.classes[{j}]"),
            Loc::Member(j, k) => format!("sections.found.classes[{j}][{k}]"),
        }
    }
}

/// The first violation (§8.8), in this order: F-1 the shard's phase is
/// `map`; F-2 it has `inputs`; then for each place in turn F-3 its path is
/// absolute, F-4 above the one before, F-5 its input is one of the `n`
/// inputs; then for each class in turn F-6 two inputs or more, F-7 its
/// first above the first of the class before, and for each of its inputs in
/// turn F-8 one of the `n`, F-9 above the one before, F-10 in no class
/// before.
pub(super) fn violation(
    found: &Found,
    phase: Phase,
    inputs: Option<usize>,
) -> Option<(Loc, &'static str)> {
    if phase != Phase::Map {
        return Some((Loc::Section, "a found section in a shard whose phase is not map"));
    }
    let Some(n) = inputs else {
        return Some((Loc::Section, "a found section in a shard without inputs"));
    };
    let n = n as u64;
    for (i, p) in found.places.iter().enumerate() {
        if !p.path.starts_with(b"/") {
            return Some((Loc::Place(i), "a found place is not an absolute path"));
        }
        if i > 0 && found.places[i - 1].path >= p.path {
            return Some((Loc::Place(i), "found places are not strictly ascending"));
        }
        if p.input >= n {
            return Some((Loc::Input(i), "a found place names no input"));
        }
    }
    let mut seen: HashSet<u64> = HashSet::new();
    for (j, class) in found.classes.iter().enumerate() {
        if class.len() < 2 {
            return Some((Loc::Class(j), "a class of fewer than two inputs"));
        }
        if j > 0 && found.classes[j - 1].first() >= class.first() {
            return Some((Loc::Class(j), "classes are not ascending by their first input"));
        }
        for (k, &x) in class.iter().enumerate() {
            if x >= n {
                return Some((Loc::Member(j, k), "a class names no input"));
            }
            if k > 0 && class[k - 1] >= x {
                return Some((Loc::Member(j, k), "a class is not strictly ascending"));
            }
            if !seen.insert(x) {
                return Some((Loc::Member(j, k), "an input in two classes"));
            }
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

/// `places: list<FoundPlace>`, `FoundPlace := shared: uv, rest: bytes,
/// input: uv` (each path the first `shared` bytes of the one before, then
/// `rest`, as `negatives`); `classes: list<list<uv>>`.
pub(super) fn encode(e: &mut Encoder, found: &Found) {
    e.count(found.places.len());
    let mut prev: &[u8] = &[];
    for p in &found.places {
        let shared = common(prev, &p.path);
        e.uv(shared as u64).bytes(&p.path[shared..]).uv(p.input);
        prev = &p.path;
    }
    e.count(found.classes.len());
    for class in &found.classes {
        e.count(class.len());
        for &x in class {
            e.uv(x);
        }
    }
}

/// File offsets of what an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    places: Vec<usize>,
    inputs: Vec<usize>,
    classes: Vec<usize>,
    members: Vec<Vec<usize>>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Section => self.section,
            Loc::Place(i) => self.places[i],
            Loc::Input(i) => self.inputs[i],
            Loc::Class(j) => self.classes[j],
            Loc::Member(j, k) => self.members[j][k],
        }
    }
}

/// The section, and where each part begins. The front-coding's own rules
/// are checked here, at the `shared` field, as `negatives` checks them.
pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Found, Offsets)> {
    let mut offsets = Offsets { section: d.offset(), ..Offsets::default() };
    let place_count = d.count()?;
    let mut places: Vec<FoundPlace> = Vec::with_capacity(place_count.min(1 << 16));
    for _ in 0..place_count {
        let at = d.offset();
        offsets.places.push(at);
        let shared = d.uv()?;
        let rest = d.bytes()?;
        let prev: &[u8] = places.last().map_or(&[], |p| p.path.as_slice());
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
        offsets.inputs.push(d.offset());
        let input = d.uv()?;
        places.push(FoundPlace { path, input });
    }
    let class_count = d.count()?;
    let mut classes: Vec<Vec<u64>> = Vec::with_capacity(class_count.min(1 << 16));
    for _ in 0..class_count {
        offsets.classes.push(d.offset());
        let size = d.count()?;
        let mut class = Vec::with_capacity(size.min(1 << 16));
        let mut at = Vec::with_capacity(size.min(1 << 16));
        for _ in 0..size {
            at.push(d.offset());
            class.push(d.uv()?);
        }
        offsets.members.push(at);
        classes.push(class);
    }
    Ok((Found { places, classes }, offsets))
}

// ---------------------------------------------------------------------------
// JSON

/// `{ "places": [{ "path", "input" }…], "classes": [[…]…] }`, the paths
/// whole (the front-coding is the binary form's).
pub(super) fn render(found: &Found) -> Value {
    let places = found
        .places
        .iter()
        .map(|p| object(vec![("path", json_bytes(&p.path)), ("input", json_uv(p.input))]))
        .collect();
    let classes = found
        .classes
        .iter()
        .map(|c| Value::Array(c.iter().map(|&x| json_uv(x)).collect()))
        .collect();
    object(vec![("places", Value::Array(places)), ("classes", Value::Array(classes))])
}

pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Found> {
    let [places, classes] = read_record(v, at, ["places", "classes"])?;
    let places = read_list(places, &at.key("places"), |v, at| {
        let [path, input] = read_record(v, at, ["path", "input"])?;
        Ok(FoundPlace {
            path: read_bytes(path, &at.key("path"))?,
            input: read_uv(input, &at.key("input"))?,
        })
    })?;
    let classes = read_list(classes, &at.key("classes"), |v, at| read_list(v, at, read_uv))?;
    Ok(Found { places, classes })
}
