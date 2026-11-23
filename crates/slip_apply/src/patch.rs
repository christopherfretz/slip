//! The patch (`spec/edits.md` §7.2): the overlay as a unified diff.
//!
//! Lines are compared as bytes, each with its `\n` (a last line without
//! one differs from the same text with one). The diff is a common
//! subsequence of lines: the common prefix and suffix, then lines unique
//! on both sides as anchors (their longest increasing run), then, between
//! anchors where there are none, Myers' algorithm, bounded: a region whose
//! search would pass [`MYERS_WORK`] is written as removed and added whole.
//! Any common subsequence gives a patch that applies; the choices only
//! make it shorter (`spec/landed/edits/WP-063.md` §3).

use std::collections::HashMap;
use std::io::{self, Write};

use slip_model::Digest;
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Overlay};
use slip_model::transform::is_canonical;

use crate::{ApplyError, Files};

/// Lines of context around a change.
pub const CONTEXT: usize = 3;

/// The most work (lines times edit distance) Myers' search may do on one
/// region before the region is written whole.
pub const MYERS_WORK: usize = 1 << 26;

/// The most cells (one `isize` each, so 32 MiB) Myers' search may hold
/// for one region: its frontier and the rows it keeps to trace the script
/// back. A region that would need more is written whole, as past
/// [`MYERS_WORK`]. With `d` the edit distance reached, the cells held are
/// at most `(d + 1)^2 + 2d + 3`.
pub const MYERS_CELLS: usize = 1 << 22;

/// The lines of `bytes`, each with its `\n`; the last may have none.
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let end = bytes[at..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |i| at + i + 1);
        out.push(&bytes[at..end]);
        at = end;
    }
    out
}

/// The pairs `(i, j)` of a common subsequence of `a` and `b`, ascending.
pub(crate) fn common(a: &[u32], b: &[u32]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut work = vec![(0, a.len(), 0, b.len())];
    while let Some((mut alo, mut ahi, mut blo, mut bhi)) = work.pop() {
        while alo < ahi && blo < bhi && a[alo] == b[blo] {
            out.push((alo, blo));
            alo += 1;
            blo += 1;
        }
        while alo < ahi && blo < bhi && a[ahi - 1] == b[bhi - 1] {
            ahi -= 1;
            bhi -= 1;
            out.push((ahi, bhi));
        }
        if alo == ahi || blo == bhi {
            continue;
        }
        let anchors = unique_anchors(&a[alo..ahi], &b[blo..bhi]);
        if anchors.is_empty() {
            out.extend(
                myers(&a[alo..ahi], &b[blo..bhi]).into_iter().map(|(i, j)| (alo + i, blo + j)),
            );
            continue;
        }
        let (mut pa, mut pb) = (alo, blo);
        for (i, j) in anchors {
            let (i, j) = (alo + i, blo + j);
            out.push((i, j));
            work.push((pa, i, pb, j));
            (pa, pb) = (i + 1, j + 1);
        }
        work.push((pa, ahi, pb, bhi));
    }
    out.sort_unstable();
    debug_assert!(
        out.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1),
        "a common subsequence"
    );
    out
}

/// Lines that occur once in `a` and once in `b`, paired, then the longest
/// run of them increasing on both sides.
fn unique_anchors(a: &[u32], b: &[u32]) -> Vec<(usize, usize)> {
    let mut seen: HashMap<u32, (u32, usize, u32, usize)> = HashMap::new();
    for (i, &x) in a.iter().enumerate() {
        let s = seen.entry(x).or_insert((0, i, 0, 0));
        s.0 += 1;
    }
    for (j, &x) in b.iter().enumerate() {
        if let Some(s) = seen.get_mut(&x) {
            s.2 += 1;
            s.3 = j;
        }
    }
    let mut pairs: Vec<(usize, usize)> =
        seen.values().filter(|s| s.0 == 1 && s.2 == 1).map(|s| (s.1, s.3)).collect();
    pairs.sort_unstable();
    // Longest increasing subsequence of the `b` indices (patience sorting).
    let mut tails: Vec<usize> = Vec::new(); // index into pairs
    let mut back: Vec<Option<usize>> = vec![None; pairs.len()];
    for k in 0..pairs.len() {
        let pos = tails.partition_point(|&t| pairs[t].1 < pairs[k].1);
        back[k] = pos.checked_sub(1).map(|p| tails[p]);
        if pos == tails.len() {
            tails.push(k);
        } else {
            tails[pos] = k;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(k) = at {
        out.push(pairs[k]);
        at = back[k];
    }
    out.reverse();
    out
}

/// Myers' greedy shortest edit script, as the matched pairs; empty when
/// the search would pass [`MYERS_WORK`] or [`MYERS_CELLS`].
fn myers(a: &[u32], b: &[u32]) -> Vec<(usize, usize)> {
    myers_counted(a, b).0
}

/// [`myers`], and the most cells it held at once.
#[allow(clippy::many_single_char_names, reason = "the names of the definitions")]
fn myers_counted(a: &[u32], b: &[u32]) -> (Vec<(usize, usize)>, usize) {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = n + m;
    // The greatest edit distance searched: the bound of the work, and the
    // one of the cells (the rows kept up to `d` are `(d + 1)^2` cells,
    // the frontier `2d + 3`).
    let by_work = MYERS_WORK / (n + m).max(1) as usize;
    let mut by_cells = 0usize;
    while (by_cells + 2) * (by_cells + 2) + 2 * (by_cells + 1) + 3 <= MYERS_CELLS {
        by_cells += 1;
    }
    let dlim = (max as usize).min(by_work).min(by_cells) as isize;
    let off = dlim + 1;
    let mut v = vec![0isize; 2 * dlim as usize + 3];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut kept = v.len();
    let mut peak = kept;
    let mut done = None;
    'search: for d in 0..=dlim {
        for k in (-d..=d).step_by(2) {
            let i = (k + off) as usize;
            let mut x =
                if k == -d || (k != d && v[i - 1] < v[i + 1]) { v[i + 1] } else { v[i - 1] + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[i] = x;
            if x >= n && y >= m {
                done = Some(d);
            }
            if done.is_some() {
                break;
            }
        }
        trace.push(v[(off - d) as usize..=(off + d) as usize].to_vec());
        kept += 2 * d as usize + 1;
        peak = peak.max(kept);
        if done.is_some() {
            break 'search;
        }
    }
    let Some(dmax) = done else { return (Vec::new(), peak) };
    let mut out = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..=dmax).rev() {
        let prev = &trace[d as usize - 1];
        let at = |k: isize| prev[(k + d - 1) as usize];
        let k = x - y;
        let pk = if k == -d || (k != d && at(k - 1) < at(k + 1)) { k + 1 } else { k - 1 };
        let px = at(pk);
        let py = px - pk;
        let (sx, sy) = if pk == k + 1 { (px, py + 1) } else { (px + 1, py) };
        while x > sx && y > sy {
            x -= 1;
            y -= 1;
            out.push((x as usize, y as usize));
        }
        (x, y) = (px, py);
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        out.push((x as usize, y as usize));
    }
    out.reverse();
    (out, peak)
}

/// One run of changed lines: `a[alo..ahi]` replaced by `b[blo..bhi]`.
#[derive(Clone, Copy, Debug)]
struct Change {
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
}

fn changes(n: usize, m: usize, matched: &[(usize, usize)]) -> Vec<Change> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for &(mi, mj) in matched.iter().chain(std::iter::once(&(n, m))) {
        if mi > i || mj > j {
            out.push(Change { alo: i, ahi: mi, blo: j, bhi: mj });
        }
        (i, j) = (mi + 1, mj + 1);
    }
    out
}

fn range(start: usize, count: usize) -> String {
    match count {
        0 => format!("{start},0"),
        1 => format!("{}", start + 1),
        _ => format!("{},{count}", start + 1),
    }
}

fn line(out: &mut dyn Write, mark: u8, l: &[u8]) -> io::Result<()> {
    out.write_all(&[mark])?;
    out.write_all(l)?;
    if l.last() != Some(&b'\n') {
        out.write_all(b"\n\\ No newline at end of file\n")?;
    }
    Ok(())
}

/// The file section of one file: its two header lines and its hunks.
pub(crate) fn file_diff(
    name: &[u8],
    pre: &[u8],
    post: &[u8],
    out: &mut dyn Write,
) -> io::Result<()> {
    let (a, b) = (lines(pre), lines(post));
    let mut ids: HashMap<&[u8], u32> = HashMap::new();
    let mut ai = Vec::with_capacity(a.len());
    let mut bi = Vec::with_capacity(b.len());
    for (from, to) in [(&a, &mut ai), (&b, &mut bi)] {
        for &l in from {
            let next = ids.len() as u32;
            to.push(*ids.entry(l).or_insert(next));
        }
    }
    let all = changes(a.len(), b.len(), &common(&ai, &bi));
    if all.is_empty() {
        return Ok(());
    }
    // A name with a space ends at a tab, as diff(1) ends one before its
    // timestamp: patch(1) would otherwise end it at the space.
    let end: &[u8] = if name.contains(&b' ') { b"\t\n" } else { b"\n" };
    for mark in [&b"--- "[..], b"+++ "] {
        out.write_all(mark)?;
        out.write_all(name)?;
        out.write_all(end)?;
    }
    let mut k = 0;
    while k < all.len() {
        // The changes of one hunk: while the context of the next touches or
        // overlaps this one's.
        let mut last = k;
        while last + 1 < all.len() && all[last + 1].alo - all[last].ahi <= 2 * CONTEXT {
            last += 1;
        }
        let first = all[k];
        let end = all[last];
        let lead = first.alo.min(CONTEXT);
        let trail = (a.len() - end.ahi).min(CONTEXT);
        let (sa, sb) = (first.alo - lead, first.blo - lead);
        let (ea, eb) = (end.ahi + trail, end.bhi + trail);
        out.write_all(
            format!("@@ -{} +{} @@\n", range(sa, ea - sa), range(sb, eb - sb)).as_bytes(),
        )?;
        let mut at = sa;
        for c in &all[k..=last] {
            for l in &a[at..c.alo] {
                line(out, b' ', l)?;
            }
            for l in &a[c.alo..c.ahi] {
                line(out, b'-', l)?;
            }
            for l in &b[c.blo..c.bhi] {
                line(out, b'+', l)?;
            }
            at = c.ahi;
        }
        for l in &a[at..ea] {
            line(out, b' ', l)?;
        }
        k = last + 1;
    }
    Ok(())
}

/// The name a file section gives a path: the path, or under `relative_to`
/// the rest of it after that directory. `Invalid` for a path outside the
/// directory, and for a name with a tab, a carriage return or a newline,
/// that starts with `"` (which both tools read as a quoted name), or that
/// starts or ends with a space (GNU `patch` drops it): a unified diff's
/// header cannot carry them.
fn name_of<'p>(path: &'p [u8], relative_to: Option<&[u8]>) -> Result<&'p [u8], ApplyError> {
    let name = match relative_to {
        None => path,
        Some(b"/") => &path[1..],
        Some(dir) => match path.strip_prefix(dir) {
            Some([b'/', rest @ ..]) => rest,
            _ => {
                return Err(ApplyError::Invalid(format!(
                    "{} is not inside the directory the patch is relative to",
                    String::from_utf8_lossy(path)
                )));
            }
        },
    };
    if name.iter().any(|b| matches!(b, b'\t' | b'\r' | b'\n'))
        || name.first() == Some(&b'"')
        || name.first() == Some(&b' ')
        || name.last() == Some(&b' ')
    {
        return Err(ApplyError::Invalid(format!(
            "{:?}: a name with a tab, a carriage return or a newline, that starts with a \
             quote, or that starts or ends with a space, cannot be written in a patch's header",
            String::from_utf8_lossy(path)
        )));
    }
    Ok(name)
}

fn is_image(bytes: &[u8], c: &Content) -> bool {
    bytes.len() as u64 == c.size && content_digest(bytes) == c.content
}

/// §7.2: the overlay as a unified diff, a function of the overlay and the
/// pre-image bytes. Every name is checked before anything is written.
pub fn patch(
    overlay: &Overlay,
    pre: &dyn Files,
    post: &dyn Fn(Digest) -> io::Result<Vec<u8>>,
    relative_to: Option<&[u8]>,
    out: &mut dyn Write,
) -> Result<(), ApplyError> {
    overlay.validate().map_err(ApplyError::Invalid)?;
    if let Some(dir) = relative_to
        && !is_canonical(dir)
    {
        return Err(ApplyError::Invalid(format!(
            "{}: the directory a patch is relative to is not a canonical absolute path",
            String::from_utf8_lossy(dir)
        )));
    }
    let changed: Vec<_> = overlay.files.iter().filter(|f| f.pre != f.post).collect();
    let mut names = Vec::with_capacity(changed.len());
    for f in &changed {
        names.push(name_of(&f.path, relative_to)?);
    }
    for (f, name) in changed.iter().zip(names) {
        let a = match pre.read(&f.path) {
            Ok(a) if is_image(&a, &f.pre) => a,
            _ => return Err(ApplyError::StaleFile(f.path.clone())),
        };
        let b = match post(f.post.content) {
            Ok(b) if is_image(&b, &f.post) => b,
            Ok(_) => {
                return Err(ApplyError::Io(
                    f.path.clone(),
                    "the post-image blob does not hash to its name".to_string(),
                ));
            }
            Err(e) => return Err(ApplyError::Io(f.path.clone(), e.to_string())),
        };
        file_diff(name, &a, &b, out).map_err(|e| ApplyError::Io(f.path.clone(), e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MYERS_CELLS, myers_counted};

    // spec/landed/edits/WP-063.md §3: the cells Myers holds are bounded by
    // MYERS_CELLS, the case of the review (4096 lines against 4096 other
    // lines) included; within the bound the script is still found.
    #[test]
    fn myers_memory_is_bounded() {
        for n in [16usize, 1000, 2047, 4096, 20_000] {
            let a = vec![1u32; n];
            let b = vec![2u32; n];
            let (pairs, peak) = myers_counted(&a, &b);
            assert!(pairs.is_empty());
            assert!(peak <= MYERS_CELLS, "{n} lines: {peak} cells");
        }
        // Repeated lines with a few differences: found, and bounded.
        let a: Vec<u32> = (0..3000).map(|i| i % 2).collect();
        let mut b = a.clone();
        b.remove(1500);
        b.insert(10, 7);
        let (pairs, peak) = myers_counted(&a, &b);
        assert_eq!(pairs.len(), 2999);
        assert!(peak <= MYERS_CELLS);
    }
}
