//! The geometry of `spec/edits.md` §5.4: `contested` by another range,
//! `overlap`, `cyclic`; and the containment forest §5.5's effect walks.
//!
//! Ranges of every file are put on one line: a range `[lo, hi)` of the
//! file of rank `p` is `[p·2^64 + lo, p·2^64 + hi)` (a `u128`), so that
//! ranges of different files share no byte and one sorted list serves
//! every file. Every query is a sweep or a segment tree over the sorted
//! ranges, never a comparison of pairs. The predicates are those of
//! `slip_model::editset` (`spec/landed/edits/WP-061.md` §1), on `u128`.

use std::collections::BTreeMap;

/// A range on the line.
pub(crate) type Span = (u128, u128);

/// The point of offset `v` of the file of rank `pid`.
pub(crate) fn at(pid: u32, v: u64) -> u128 {
    (u128::from(pid) << 64) | u128::from(v)
}

pub(crate) fn share_byte(a: Span, b: Span) -> bool {
    a.0 < a.1 && b.0 < b.1 && a.0 < b.1 && b.0 < a.1
}

pub(crate) fn within(a: Span, b: Span) -> bool {
    a.0 < a.1 && b.0 <= a.0 && a.1 <= b.1
}

pub(crate) fn properly_inside(a: Span, b: Span) -> bool {
    within(a, b) && a != b
}

/// A static segment tree of maxima, for "the first index in `[l, r)`
/// whose value is above `t`".
struct MaxTree {
    size: usize,
    t: Vec<u128>,
}

impl MaxTree {
    fn new(values: impl ExactSizeIterator<Item = u128>) -> MaxTree {
        let n = values.len();
        let size = n.next_power_of_two().max(1);
        let mut t = vec![0u128; 2 * size];
        for (i, v) in values.enumerate() {
            t[size + i] = v;
        }
        for k in (1..size).rev() {
            t[k] = t[2 * k].max(t[2 * k + 1]);
        }
        MaxTree { size, t }
    }

    /// The least index `i` in `[l, r)` with `value[i] > t`.
    fn first_above(&self, l: usize, r: usize, t: u128) -> Option<usize> {
        if l >= r {
            return None;
        }
        self.descend(1, 0, self.size, l, r, t)
    }

    fn descend(
        &self,
        k: usize,
        nl: usize,
        nr: usize,
        l: usize,
        r: usize,
        t: u128,
    ) -> Option<usize> {
        if nr <= l || r <= nl || self.t[k] <= t {
            return None;
        }
        if nr - nl == 1 {
            return Some(nl);
        }
        let mid = nl.midpoint(nr);
        self.descend(2 * k, nl, mid, l, r, t).or_else(|| self.descend(2 * k + 1, mid, nr, l, r, t))
    }
}

/// A segment tree of minima with point updates.
struct MinTree {
    size: usize,
    t: Vec<usize>,
}

impl MinTree {
    fn new(n: usize) -> MinTree {
        let size = n.next_power_of_two().max(1);
        MinTree { size, t: vec![usize::MAX; 2 * size] }
    }

    fn lower(&mut self, i: usize, v: usize) {
        let mut k = self.size + i;
        self.t[k] = self.t[k].min(v);
        while k > 1 {
            k /= 2;
            self.t[k] = self.t[2 * k].min(self.t[2 * k + 1]);
        }
    }

    /// The least value over `[l, r)`.
    fn min(&self, l: usize, r: usize) -> usize {
        let (mut l, mut r) = (l + self.size, r + self.size);
        let mut m = usize::MAX;
        while l < r {
            if l & 1 == 1 {
                m = m.min(self.t[l]);
                l += 1;
            }
            if r & 1 == 1 {
                r -= 1;
                m = m.min(self.t[r]);
            }
            l /= 2;
            r /= 2;
        }
        m
    }
}

/// For each query, the least range of `family` (sorted by `(lo, hi)`, every
/// range with a byte) that crosses it, as an index into `family`.
///
/// `b` crosses `a` iff `b.lo < a.lo < b.hi < a.hi` (a "left" crossing) or
/// `a.lo < b.lo < a.hi < b.hi` (a "right" one). Every left one is below
/// every right one in `(lo, hi)`, so the least is the least left one if
/// there is one. Right: the first index with `lo` in `(a.lo, a.hi)` whose
/// `hi` is above `a.hi`, from a tree of maxima. Left: offline, queries by
/// `lo`, the ranges with `lo < a.lo` inserted into a tree of minima over
/// the ranks of their `hi`, asked for the least index with `hi` in
/// `(a.lo, a.hi)`.
pub(crate) fn least_crossing(family: &[Span], queries: &[Span]) -> Vec<Option<usize>> {
    let mut out = vec![None; queries.len()];
    if family.is_empty() {
        return out;
    }
    let his = MaxTree::new(family.iter().map(|r| r.1));
    for (q, a) in queries.iter().enumerate() {
        let l = family.partition_point(|b| b.0 <= a.0);
        let r = family.partition_point(|b| b.0 < a.1);
        out[q] = his.first_above(l, r, a.1);
    }
    let mut ranks: Vec<u128> = family.iter().map(|r| r.1).collect();
    ranks.sort_unstable();
    ranks.dedup();
    let mut tree = MinTree::new(ranks.len());
    let mut order: Vec<usize> = (0..queries.len()).collect();
    order.sort_by_key(|&q| queries[q].0);
    let mut next = 0;
    for q in order {
        let a = queries[q];
        while next < family.len() && family[next].0 < a.0 {
            let rank = ranks.partition_point(|&h| h < family[next].1);
            tree.lower(rank, next);
            next += 1;
        }
        let l = ranks.partition_point(|&h| h <= a.0);
        let r = ranks.partition_point(|&h| h < a.1);
        if l < r {
            let m = tree.min(l, r);
            if m != usize::MAX {
                out[q] = Some(m);
            }
        }
    }
    out
}

/// For each query, the least range of `blocking` (sorted by `(lo, hi)`)
/// that shares a byte with it and is not the query itself.
pub(crate) fn least_sharing(blocking: &[Span], queries: &[Span]) -> Vec<Option<usize>> {
    let his = MaxTree::new(blocking.iter().map(|r| r.1));
    queries
        .iter()
        .map(|&a| {
            // Ranges with `lo < a.hi` and `hi > a.lo` share a byte with `a`
            // (every range here has one). They are a prefix by `lo`.
            let r = blocking.partition_point(|b| b.0 < a.1);
            let i = his.first_above(0, r, a.0)?;
            if blocking[i] == a { his.first_above(i + 1, r, a.0) } else { Some(i) }
        })
        .collect()
}

/// The indices of `[lo, hi)` within `sorted` (by `(lo, hi)`) of the ranges
/// properly inside `r`, given that none crosses `r`: two intervals, the
/// ranges that start at `r.lo` and are shorter than `r`, and those that
/// start after `r.lo` and before `r.hi`.
#[allow(clippy::many_single_char_names)]
pub(crate) fn properly_inside_of(sorted: &[Span], r: Span) -> [(usize, usize); 2] {
    let a = sorted.partition_point(|x| x.0 < r.0);
    let b = sorted.partition_point(|x| x.0 < r.1);
    let c = a + sorted[a..b].partition_point(|x| x.0 == r.0 && x.1 < r.1);
    let d = c + sorted[c..b].partition_point(|x| x.0 == r.0);
    [(a, c), (d, b)]
}

/// The innermost range of `sorted` (by `(lo, hi)`, no two crossing, no two
/// equal) that properly contains each range, for the ranges that have one.
/// One sweep with a stack of the open ranges, each block of ranges with
/// one `lo` taken from the longest down.
pub(crate) fn parents(sorted: &[Span]) -> BTreeMap<usize, usize> {
    let mut parent = BTreeMap::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let lo = sorted[i].0;
        let j = i + sorted[i..].partition_point(|x| x.0 == lo);
        for k in (i..j).rev() {
            while stack.last().is_some_and(|&t| sorted[t].1 <= sorted[k].0) {
                stack.pop();
            }
            if let Some(&t) = stack.last() {
                parent.insert(k, t);
            }
            stack.push(k);
        }
        i = j;
    }
    parent
}

/// `cyclic` (§5.4): the edits (indices into `edits`, sorted by `(lo, hi)`,
/// no two crossing and none crossing a copied range) on a cycle of "draws
/// on": `e` draws on `x` when `x` is properly inside a range `e` copies.
///
/// The graph has a node per edit within some copied range (the only ones
/// drawn on) and per copying edit, and a segment tree over the former, so
/// that a copy is O(log n) edges to the contiguous blocks it draws on
/// whatever their size. An edit is on a cycle iff its strongly connected
/// component has more than one node (a tree node never closes a cycle).
pub(crate) fn cyclic(edits: &[Span], copies: &[(usize, Span)]) -> Vec<usize> {
    if copies.is_empty() {
        return Vec::new();
    }
    // The union of the copied ranges, and the edits within it.
    let mut ranges: Vec<Span> = copies.iter().map(|c| c.1).collect();
    ranges.sort_unstable();
    let mut merged: Vec<Span> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(m) if r.0 <= m.1 => m.1 = m.1.max(r.1),
            _ => merged.push(r),
        }
    }
    let targets: Vec<usize> = (0..edits.len())
        .filter(|&i| {
            let e = edits[i];
            let k = merged.partition_point(|m| m.0 <= e.0);
            k > 0 && within(e, merged[k - 1])
        })
        .collect();
    let target_spans: Vec<Span> = targets.iter().map(|&i| edits[i]).collect();

    // Nodes: edits first (targets and copying edits), then the tree.
    let mut node_of: BTreeMap<usize, usize> = BTreeMap::new();
    for &i in &targets {
        let n = node_of.len();
        node_of.entry(i).or_insert(n);
    }
    for &(e, _) in copies {
        let n = node_of.len();
        node_of.entry(e).or_insert(n);
    }
    let m = node_of.len();
    let size = target_spans.len().next_power_of_two().max(1);
    let tree = |k: usize| m + k;
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); m + 2 * size];
    for k in 1..size {
        adj[tree(k)].push(tree(2 * k));
        adj[tree(k)].push(tree(2 * k + 1));
    }
    for (t, &i) in targets.iter().enumerate() {
        adj[tree(size + t)].push(node_of[&i]);
    }
    for &(e, r) in copies {
        let from = node_of[&e];
        for (l, h) in properly_inside_of(&target_spans, r) {
            let (mut l, mut h) = (l + size, h + size);
            while l < h {
                if l & 1 == 1 {
                    adj[from].push(tree(l));
                    l += 1;
                }
                if h & 1 == 1 {
                    h -= 1;
                    adj[from].push(tree(h));
                }
                l /= 2;
                h /= 2;
            }
        }
    }
    let component = scc(&adj);
    let mut size_of: BTreeMap<usize, usize> = BTreeMap::new();
    for &c in &component {
        *size_of.entry(c).or_default() += 1;
    }
    let mut on_cycle: Vec<usize> =
        node_of.iter().filter(|&(_, &n)| size_of[&component[n]] > 1).map(|(&i, _)| i).collect();
    on_cycle.sort_unstable();
    on_cycle
}

/// Tarjan's strongly connected components, without recursion: the
/// component number of each node.
fn scc(adj: &[Vec<usize>]) -> Vec<usize> {
    const NONE: usize = usize::MAX;
    let n = adj.len();
    let mut index = vec![NONE; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut component = vec![NONE; n];
    let mut next_index = 0;
    let mut next_component = 0;
    let mut calls: Vec<(usize, usize)> = Vec::new();
    for root in 0..n {
        if index[root] != NONE {
            continue;
        }
        calls.push((root, 0));
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (v, ref mut pos)) = calls.last_mut() {
            if let Some(&w) = adj[v].get(*pos) {
                *pos += 1;
                if index[w] == NONE {
                    index[w] = next_index;
                    low[w] = next_index;
                    next_index += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    calls.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                calls.pop();
                if let Some(&(u, _)) = calls.last() {
                    low[u] = low[u].min(low[v]);
                }
                if low[v] == index[v] {
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        component[w] = next_component;
                        if w == v {
                            break;
                        }
                    }
                    next_component += 1;
                }
            }
        }
    }
    component
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crosses(a: Span, b: Span) -> bool {
        share_byte(a, b) && !within(a, b) && !within(b, a)
    }

    struct Lcg(u64);
    impl Lcg {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % n
        }
    }

    fn spans(r: &mut Lcg, n: usize, width: u64) -> Vec<Span> {
        let mut v: Vec<Span> = (0..n)
            .map(|_| {
                let lo = r.below(width);
                let hi = lo + 1 + r.below(width / 4 + 1);
                (u128::from(lo), u128::from(hi))
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    // edits.md §5.4 `overlap`: the sweep finds the least crossing range,
    // as a comparison of every pair does.
    #[test]
    fn least_crossing_agrees_with_every_pair() {
        let mut r = Lcg(7);
        for _ in 0..300 {
            let family = {
                let k = 1 + r.below(30) as usize;
                spans(&mut r, k, 40)
            };
            let queries = {
                let k = 1 + r.below(30) as usize;
                spans(&mut r, k, 40)
            };
            let got = least_crossing(&family, &queries);
            for (q, a) in queries.iter().enumerate() {
                let want = (0..family.len()).find(|&i| crosses(family[i], *a));
                assert_eq!(got[q], want, "family {family:?} query {a:?}");
            }
        }
    }

    // edits.md §5.4 `contested`: the least range sharing a byte, not itself.
    #[test]
    fn least_sharing_agrees_with_every_pair() {
        let mut r = Lcg(11);
        for _ in 0..300 {
            let blocking = {
                let k = r.below(30) as usize;
                spans(&mut r, k, 40)
            };
            let mut queries = {
                let k = 1 + r.below(10) as usize;
                spans(&mut r, k, 40)
            };
            queries.extend(blocking.iter().take(3));
            let got = least_sharing(&blocking, &queries);
            for (q, a) in queries.iter().enumerate() {
                let want =
                    (0..blocking.len()).find(|&i| blocking[i] != *a && share_byte(blocking[i], *a));
                assert_eq!(got[q], want, "blocking {blocking:?} query {a:?}");
            }
        }
    }

    // edits.md §5.5: the innermost container, as a comparison of pairs gives.
    #[test]
    fn parents_agree_with_every_pair() {
        let mut r = Lcg(13);
        for _ in 0..300 {
            // A laminar family: drop every range that crosses an earlier one.
            let mut family: Vec<Span> = Vec::new();
            for s in {
                let k = 1 + r.below(30) as usize;
                spans(&mut r, k, 40)
            } {
                if family.iter().all(|&f| !crosses(f, s)) {
                    family.push(s);
                }
            }
            family.sort_unstable();
            let got = parents(&family);
            for (i, &x) in family.iter().enumerate() {
                let want = (0..family.len())
                    .filter(|&j| properly_inside(x, family[j]))
                    .min_by_key(|&j| family[j].1 - family[j].0);
                assert_eq!(got.get(&i).copied(), want, "{family:?} at {x:?}");
            }
        }
    }

    // edits.md §5.4 `cyclic`, against a closure over every pair.
    #[test]
    fn cyclic_agrees_with_a_closure() {
        let mut r = Lcg(17);
        for _ in 0..300 {
            let mut family: Vec<Span> = Vec::new();
            for s in {
                let k = 1 + r.below(12) as usize;
                spans(&mut r, k, 30)
            } {
                if family.iter().all(|&f| !crosses(f, s)) {
                    family.push(s);
                }
            }
            family.sort_unstable();
            let mut copies: Vec<(usize, Span)> = Vec::new();
            for _ in 0..r.below(6) {
                let e = r.below(family.len() as u64) as usize;
                let c = spans(&mut r, 1, 30)[0];
                if family.iter().all(|&f| !crosses(f, c)) {
                    copies.push((e, c));
                }
            }
            let n = family.len();
            let mut reach = vec![vec![false; n]; n];
            for &(e, c) in &copies {
                for x in 0..n {
                    if properly_inside(family[x], c) {
                        reach[e][x] = true;
                    }
                }
            }
            for k in 0..n {
                for i in 0..n {
                    for j in 0..n {
                        if reach[i][k] && reach[k][j] {
                            reach[i][j] = true;
                        }
                    }
                }
            }
            let want: Vec<usize> = (0..n).filter(|&i| reach[i][i]).collect();
            assert_eq!(cyclic(&family, &copies), want, "{family:?} {copies:?}");
        }
    }
}
