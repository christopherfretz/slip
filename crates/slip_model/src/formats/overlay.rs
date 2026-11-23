//! The overlay, kind `OVLY` (`spec/edits.md` §7.1, `spec/verify.md` §10.1).
//! Work package WP-061; the order of checks, the messages and the JSON form
//! are `spec/landed/edits/WP-061.md` §3.

use crate::Digest;
use crate::json::Value;
use crate::overlay::{Content, Obligation, Overlay, OverlayFile};
use crate::transform::is_canonical;
use crate::wire::{Decoder, Encoder};

use super::common::{
    JResult, Result, dec_list, json_bytes, json_digest, json_uv, malformed, read_bytes,
    read_digest, read_list, read_record, read_uv,
};
use super::edits::record;
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, open, read_container, render_container,
    seal,
};

/// The section id (§7.1).
const OVERLAY: u64 = 1;

#[derive(Clone, Copy, Debug)]
enum Loc {
    Files,
    File(usize),
    Spelling(usize),
}

impl Loc {
    fn path(self) -> String {
        const S: &str = "sections.overlay";
        match self {
            Loc::Files => format!("{S}.files"),
            Loc::File(i) => format!("{S}.files[{i}]"),
            Loc::Spelling(i) => format!("{S}.spellings[{i}]"),
        }
    }
}

/// The first violation, each rule over its whole list before the next:
/// `files` not empty, canonical, strictly ascending by path; `spellings`
/// strictly ascending, each `file` an index of `files`, `lo < hi <=
/// post.size`.
fn violation(x: &Overlay) -> Option<(Loc, &'static str)> {
    if x.files.is_empty() {
        return Some((Loc::Files, "an overlay has no file"));
    }
    if let Some(i) = x.files.iter().position(|f| !is_canonical(&f.path)) {
        return Some((Loc::File(i), "not a canonical absolute path"));
    }
    if let Some(i) = (1..x.files.len()).find(|&i| x.files[i - 1].path >= x.files[i].path) {
        return Some((Loc::File(i), "files are not strictly ascending by path"));
    }
    let s = &x.spellings;
    if let Some(i) = (1..s.len()).find(|&i| s[i - 1] >= s[i]) {
        return Some((
            Loc::Spelling(i),
            "spellings are not strictly ascending by (file, lo, hi, key)",
        ));
    }
    let size = |o: &Obligation| {
        usize::try_from(o.file).ok().and_then(|i| x.files.get(i)).map(|f| f.post.size)
    };
    if let Some(i) = s.iter().position(|o| size(o).is_none()) {
        return Some((Loc::Spelling(i), "an obligation's file is not the index of a file"));
    }
    if let Some(i) = s.iter().position(|o| !(o.lo < o.hi && size(o).is_some_and(|n| o.hi <= n))) {
        return Some((
            Loc::Spelling(i),
            "an obligation's range is empty or not within its post-image",
        ));
    }
    None
}

pub(super) fn check(x: &Overlay) -> std::result::Result<(), (String, &'static str)> {
    violation(x).map_or(Ok(()), |(l, what)| Err((l.path(), what)))
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(x: &Overlay, extras: &[Extra<'_>]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.digest(&x.edits).count(x.files.len());
    for f in &x.files {
        e.bytes(&f.path)
            .uv(f.pre.size)
            .digest(&f.pre.content)
            .uv(f.post.size)
            .digest(&f.post.content);
    }
    e.count(x.spellings.len());
    for o in &x.spellings {
        e.uv(o.file).uv(o.lo).uv(o.hi).digest(&o.key);
    }
    seal(Overlay::KIND, Overlay::VERSION, &[(OVERLAY, e.as_bytes())], extras)
}

fn dec_content(d: &mut Decoder<'_>) -> Result<Content> {
    Ok(Content { size: d.uv()?, content: d.digest()? })
}

/// The overlay, its unknown ignorable sections and its content id.
pub(super) fn decode(bytes: &[u8]) -> Result<(Overlay, Vec<Extra<'_>>, Digest)> {
    let opened = open(bytes, Overlay::KIND, Overlay::VERSION, [OVERLAY])?;
    let [section] = opened.sections;
    let mut d = super::required(section, opened.end, "overlay has no overlay section")?;
    let edits = d.digest()?;
    let files_at = d.offset();
    let mut file_offsets = Vec::new();
    let files = dec_list(&mut d, Some(&mut file_offsets), |d| {
        Ok(OverlayFile { path: d.bytes()?.to_vec(), pre: dec_content(d)?, post: dec_content(d)? })
    })?;
    let mut spelling_offsets = Vec::new();
    let spellings = dec_list(&mut d, Some(&mut spelling_offsets), |d| {
        Ok(Obligation { file: d.uv()?, lo: d.uv()?, hi: d.uv()?, key: d.digest()? })
    })?;
    d.finish()?;
    let x = Overlay { edits, files, spellings };
    if let Some((l, what)) = violation(&x) {
        let at = match l {
            Loc::Files => files_at,
            Loc::File(i) => file_offsets[i],
            Loc::Spelling(i) => spelling_offsets[i],
        };
        return Err(malformed(at, what));
    }
    Ok((x, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 1] = [SectionName { id: OVERLAY, name: "overlay", required: true }];
const FIELDS: [&str; 3] = ["edits", "files", "spellings"];
const FILE: [&str; 3] = ["path", "pre", "post"];
const CONTENT: [&str; 2] = ["size", "content"];
const OBLIGATION: [&str; 4] = ["file", "lo", "hi", "key"];

fn json_content(c: &Content) -> Value {
    record(CONTENT, [json_uv(c.size), json_digest(&c.content)])
}

pub(super) fn render(x: &Overlay, extras: &[Extra<'_>]) -> Value {
    let files = x
        .files
        .iter()
        .map(|f| record(FILE, [json_bytes(&f.path), json_content(&f.pre), json_content(&f.post)]))
        .collect();
    let spellings = x
        .spellings
        .iter()
        .map(|o| {
            record(OBLIGATION, [json_uv(o.file), json_uv(o.lo), json_uv(o.hi), json_digest(&o.key)])
        })
        .collect();
    let overlay =
        record(FIELDS, [json_digest(&x.edits), Value::Array(files), Value::Array(spellings)]);
    render_container(Overlay::KIND, Overlay::VERSION, vec![("overlay", overlay)], extras)
}

fn read_content(v: &Value, at: &At<'_>) -> JResult<Content> {
    let [size, content] = read_record(v, at, CONTENT)?;
    Ok(Content {
        size: read_uv(size, &at.key("size"))?,
        content: read_digest(content, &at.key("content"))?,
    })
}

/// The overlay and the unknown ignorable sections of a JSON rendering.
pub(super) fn read(v: &Value) -> JResult<(Overlay, OwnedExtras)> {
    let s = read_container(v, Overlay::KIND, Overlay::VERSION, &NAMES)?;
    let [Some(section)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");
    let at = sections.key("overlay");
    let [edits, files, spellings] = read_record(section, &at, FIELDS)?;
    let x = Overlay {
        edits: read_digest(edits, &at.key("edits"))?,
        files: read_list(files, &at.key("files"), |v, at| {
            let [path, pre, post] = read_record(v, at, FILE)?;
            Ok(OverlayFile {
                path: read_bytes(path, &at.key("path"))?,
                pre: read_content(pre, &at.key("pre"))?,
                post: read_content(post, &at.key("post"))?,
            })
        })?,
        spellings: read_list(spellings, &at.key("spellings"), |v, at| {
            let [file, lo, hi, key] = read_record(v, at, OBLIGATION)?;
            Ok(Obligation {
                file: read_uv(file, &at.key("file"))?,
                lo: read_uv(lo, &at.key("lo"))?,
                hi: read_uv(hi, &at.key("hi"))?,
                key: read_digest(key, &at.key("key"))?,
            })
        })?,
    };
    if let Some((l, what)) = violation(&x) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((x, s.extras))
}
