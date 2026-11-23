//! Names of findings (`spec/verify.md` §6.1 "Interface"), the first form:
//! work package WP-052 fills `name` only; every finding's `location`
//! stays absent until `docs/wp/WP-054.md` adds traces behind the same four
//! items.
//!
//! Outside the trusted base: no verdict depends on a name (§5.2). The one
//! thing a verdict takes from here is [`Side::tu`], for guard G3.

use slip_digest::{Detail, Input, Options, ReadOnly, ReadOnlyFiles, digest};
use slip_model::Digest;
use slip_model::keys::RuleTable;
use slip_model::transform::Ownership;
use slip_model::verdict::Finding;

/// Longest `Finding::name`, in bytes (§6.1).
const NAME_MAX: usize = 4096;

/// What localisation needs from one image, taken while its parse is in
/// memory: the `tu` digest of the walk, and the names of the wanted keys
/// (every name a unit of such a key has, sorted by key then name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Side {
    tu: Digest,
    names: Vec<(Digest, String)>,
}

impl Side {
    /// The `tu` digest of the walk that took this side.
    pub fn tu(&self) -> &Digest {
        &self.tu
    }

    /// The first name, in the order of its bytes, of a unit of `key`.
    fn name(&self, key: &Digest) -> Option<&str> {
        let start = self.names.partition_point(|(k, _)| k < key);
        self.names[start..].iter().take_while(|(k, _)| k == key).map(|(_, n)| n.as_str()).min()
    }
}

/// One walk of `input` under `rules` and the run's `files` (detail
/// `none`, names asked for; read-only files as the verify job's walk of
/// the same image decides them), keeping the names of the units whose key
/// is in `keys` (sorted, no duplicates).
pub fn take(
    input: &Input<'_>,
    rules: &RuleTable,
    files: Option<&Ownership>,
    keys: &[Digest],
) -> Side {
    let read_only = ReadOnlyFiles::new(files, input.sm, input.subject_path);
    let options = Options {
        rules,
        read_only: ReadOnly::files(&read_only),
        detail: Detail::None,
        names: true,
        rule_use: false,
    };
    let output = digest(input, &options);
    let mut names: Vec<(Digest, String)> =
        output.names.into_iter().filter(|(k, _)| keys.binary_search(k).is_ok()).collect();
    names.sort();
    Side { tu: output.digests.tu, names }
}

/// The keys of the first `locate` findings, sorted, no duplicates.
pub fn wanted(findings: &[Finding], locate: usize) -> Vec<Digest> {
    let mut keys: Vec<Digest> = findings.iter().take(locate).filter_map(|f| f.key).collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// Fills `name` of the first `locate` findings (§6.1): the first name, in
/// byte order, of a unit of the key in the pre-image when it has one, else
/// in the post-image, cut to 4096 bytes at a character boundary. `pre` is
/// `None` when the pre-image was not parsed; every name then comes from
/// the post-image. Changes no other field: `location` stays absent until
/// WP-054.
pub fn fill(findings: &mut [Finding], locate: usize, pre: Option<&Side>, post: &Side) {
    for f in findings.iter_mut().take(locate) {
        let Some(key) = f.key else { continue };
        let name = pre.and_then(|p| p.name(&key)).or_else(|| post.name(&key));
        if let Some(name) = name {
            cut(name).clone_into(&mut f.name);
        }
    }
}

/// `text` cut to at most [`NAME_MAX`] bytes at a character boundary.
fn cut(text: &str) -> &str {
    let mut end = text.len().min(NAME_MAX);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use slip_model::verdict::FindingKind;

    use super::*;

    fn d(n: u8) -> Digest {
        Digest([n; 32])
    }

    fn finding(key: Option<Digest>) -> Finding {
        Finding {
            kind: FindingKind::Changed,
            key,
            file: Vec::new(),
            pre: None,
            post: None,
            name: String::new(),
            location: None,
        }
    }

    fn side(names: &[(u8, &str)]) -> Side {
        let mut names: Vec<(Digest, String)> =
            names.iter().map(|&(k, n)| (d(k), n.to_owned())).collect();
        names.sort();
        Side { tu: d(0), names }
    }

    // §6.1 "Interface": `wanted` is the keys of the first `locate`
    // findings, sorted, without duplicates; a finding without a key adds
    // none.
    #[test]
    fn s6_1_wanted_is_the_first_keys_sorted() {
        let f = [finding(Some(d(5))), finding(Some(d(2))), finding(None), finding(Some(d(5)))];
        assert_eq!(wanted(&f, 4), [d(2), d(5)]);
        assert_eq!(wanted(&f, 1), [d(5)]);
        assert!(wanted(&f, 0).is_empty());
        assert_eq!(wanted(&f, 99), [d(2), d(5)]);
    }

    // §6.1 "Names": the pre-image's name when it has one, else the
    // post-image's; of several, the first by bytes; only the first
    // `locate` findings; `location` untouched; cut to 4096 bytes at a
    // character boundary.
    #[test]
    fn s6_1_fill_names_from_pre_then_post() {
        let pre = side(&[(1, "a::f()"), (3, "z"), (3, "b")]);
        let post = side(&[(1, "post::f()"), (2, "only_post")]);
        let mut f = vec![
            finding(Some(d(1))),
            finding(Some(d(2))),
            finding(Some(d(3))),
            finding(Some(d(4))),
            finding(None),
            finding(Some(d(1))),
        ];
        fill(&mut f, 5, Some(&pre), &post);
        let names: Vec<&str> = f.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a::f()", "only_post", "b", "", "", ""]);
        assert!(f.iter().all(|f| f.location.is_none()));
        let mut g = vec![finding(Some(d(1)))];
        fill(&mut g, 1, None, &post);
        assert_eq!(g[0].name, "post::f()");
        let mut g = vec![finding(Some(d(1)))];
        fill(&mut g, 0, Some(&pre), &post);
        assert_eq!(g[0].name, "");

        let long = format!("{}é", "x".repeat(4095));
        let mut g = vec![finding(Some(d(7)))];
        fill(&mut g, 1, None, &side(&[(7, &long)]));
        assert_eq!(g[0].name, "x".repeat(4095));
        let exact = "y".repeat(4097);
        let mut g = vec![finding(Some(d(7)))];
        fill(&mut g, 1, None, &side(&[(7, &exact)]));
        assert_eq!(g[0].name, "y".repeat(4096));
    }
}
