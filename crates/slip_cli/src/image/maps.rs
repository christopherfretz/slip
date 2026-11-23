//! The map pairs of a post command and guard G5 (`spec/verify.md` §5.1,
//! §5.3): every spelling of a file the post parse can make is mapped to
//! the spelling that file has when the post tree stands in the subject's
//! place.
//!
//! Kiln applies `-fmacro-prefix-map=` and `-ffile-prefix-map=` entries
//! sorted by their old prefix in descending byte order (a stable sort),
//! the first whose old prefix is a plain string prefix of the path
//! (Kiln `kiln_source/src/manager.rs`, `set_macro_prefix_map` and
//! `remap_macro_path`): of two entries that both match, the longer.

use super::show_path;

/// One pair `(post prefix, prefix as written)`, and the word it is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pair {
    post: Vec<u8>,
    written: Vec<u8>,
    word: Vec<u8>,
}

impl Pair {
    pub(super) fn new((post, written): (Vec<u8>, Vec<u8>), word: &[u8]) -> Pair {
        Pair { post, written, word: word.to_vec() }
    }

    fn identity(&self) -> bool {
        self.post == self.written
    }
}

/// One of `a` and `b` is a prefix of the other.
fn related(a: &[u8], b: &[u8]) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Guard G5's reason for the pairs of a post command whose words are
/// `argv` (the map words not yet added), in this order:
///
/// 1. a pair other than the identity whose post prefix is empty or holds
///    `=` (no `-fmacro-prefix-map=` word can carry it);
/// 2. two pairs whose post prefixes are prefix-related, where the shorter
///    one's pair applied to the longer post prefix does not give the
///    longer one's written prefix (then a file reached by both routes is
///    spelled through one of them, whatever the order);
/// 3. a `-fmacro-prefix-map=` or `-ffile-prefix-map=` of the command's own
///    (alone, after `-Xclang` or `-Xpreprocessor`, or in a `-Wp,` list)
///    whose old prefix is prefix-related to a prefix, post or written, of
///    a pair other than the identity (slip does not compose maps).
pub(super) fn conflict(pairs: &[Pair], argv: &[Vec<u8>]) -> Option<String> {
    for p in pairs.iter().filter(|p| !p.identity()) {
        if p.post.is_empty() || p.post.contains(&b'=') {
            return Some(format!(
                "post-image: {} is rewritten to {}, which no -fmacro-prefix-map word can map back",
                show_path(&p.word),
                show_path(&p.post)
            ));
        }
    }
    for a in pairs {
        for b in pairs {
            let Some(rest) = b.post.strip_prefix(&a.post[..]) else { continue };
            if [&a.written[..], rest].concat() != b.written {
                return Some(format!(
                    "post-image: {} and {} spell the files under {} differently ({}={} and {}={})",
                    show_path(&a.word),
                    show_path(&b.word),
                    show_path(&b.post),
                    show_path(&a.post),
                    show_path(&a.written),
                    show_path(&b.post),
                    show_path(&b.written)
                ));
            }
        }
    }
    for word in argv.iter().skip(1) {
        for old in user_maps(word) {
            let hit = pairs.iter().filter(|p| !p.identity()).find_map(|p| {
                [&p.post, &p.written].into_iter().find(|x| related(old, x)).map(|x| (p, x))
            });
            if let Some((p, x)) = hit {
                return Some(format!(
                    "post-image: {} maps {}, which overlaps {} of {}; slip does not compose prefix maps",
                    show_path(word),
                    show_path(old),
                    show_path(x),
                    show_path(&p.word)
                ));
            }
        }
    }
    None
}

/// The old prefixes of the `-fmacro-prefix-map=` and `-ffile-prefix-map=`
/// options in `word` (itself, or each item of a `-Wp,` list).
fn user_maps(word: &[u8]) -> Vec<&[u8]> {
    let items: Vec<&[u8]> = match word.strip_prefix(b"-Wp,") {
        Some(list) => list.split(|&b| b == b',').collect(),
        None => vec![word],
    };
    items
        .into_iter()
        .filter_map(|item| {
            let value = item
                .strip_prefix(b"-fmacro-prefix-map=")
                .or_else(|| item.strip_prefix(b"-ffile-prefix-map="))?;
            let eq = value.iter().position(|&b| b == b'=')?;
            Some(&value[..eq])
        })
        .collect()
}

/// The pairs the post command's map words give: those other than the
/// identity, each once, longest post prefix first (descending bytes, the
/// order Kiln applies them in).
pub(super) fn emitted(pairs: &[Pair]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for p in pairs.iter().filter(|p| !p.identity()) {
        let pair = (p.post.clone(), p.written.clone());
        if !out.contains(&pair) {
            out.push(pair);
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}
