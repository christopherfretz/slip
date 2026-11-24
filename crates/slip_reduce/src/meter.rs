//! What reduce holds at once (`spec/edits.md` §5.6, landed WP-142): exact
//! counts of the things whose number §5.6 bounds, returned beside the edit
//! set as [`Peak`] and never in it.

use std::sync::atomic::{AtomicUsize, Ordering};

/// The most reduce held at once during one call (§5.6), as the counters
/// `reduce.peak.*` ([`Peak::counters`]). Informational: they depend on
/// `fan_in` and `threads`, which the edit set does not (§1 rule 3), so
/// they are beside it in [`crate::Reduced`], never in `EditSet.counters`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Peak {
    /// `reduce.peak.sections`: decoded shards (their `edits` sections) held
    /// at once, by both passes.
    pub sections: u64,
    /// `reduce.peak.files`: temporary files open at once (runs and basis).
    pub files: u64,
    /// `reduce.peak.records`: collapsed records in memory at once in one
    /// run of the first pass or one merge.
    pub records: u64,
    /// `reduce.peak.waiting_runs`: joined states of finished runs of the
    /// first pass held until the runs before them are folded, the run just
    /// finished included. At most [`crate::WINDOW_PER_THREAD`] × `threads`.
    pub waiting_runs: u64,
    /// `reduce.peak.unparsed`: the unparsed observers the edit set lists
    /// (32 bytes each, §5.6): the one thing held that grows with the
    /// observers.
    pub unparsed: u64,
}

impl Peak {
    /// The counters by name, in name order.
    pub fn counters(&self) -> [(&'static str, u64); 5] {
        [
            ("reduce.peak.files", self.files),
            ("reduce.peak.records", self.records),
            ("reduce.peak.sections", self.sections),
            ("reduce.peak.unparsed", self.unparsed),
            ("reduce.peak.waiting_runs", self.waiting_runs),
        ]
    }
}

/// A count of things held and the most of them at once.
#[derive(Debug, Default)]
pub(crate) struct Gauge {
    now: AtomicUsize,
    peak: AtomicUsize,
}

impl Gauge {
    /// One more is held until the result is dropped.
    pub fn hold(&self) -> Held<'_> {
        let n = self.now.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak.fetch_max(n, Ordering::Relaxed);
        Held(self)
    }

    /// `n` are held at once now.
    pub fn at(&self, n: usize) {
        self.peak.fetch_max(n, Ordering::Relaxed);
    }

    fn peak(&self) -> u64 {
        self.peak.load(Ordering::Relaxed) as u64
    }
}

/// One thing held of a [`Gauge`], released when dropped.
#[derive(Debug)]
pub(crate) struct Held<'a>(&'a Gauge);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.now.fetch_sub(1, Ordering::Relaxed);
    }
}

/// The gauges of one call.
#[derive(Debug, Default)]
pub(crate) struct Meter {
    pub sections: Gauge,
    pub files: Gauge,
    pub records: Gauge,
    pub waiting: Gauge,
}

impl Meter {
    pub fn peak(&self, unparsed: usize) -> Peak {
        Peak {
            sections: self.sections.peak(),
            files: self.files.peak(),
            records: self.records.peak(),
            waiting_runs: self.waiting.peak(),
            unparsed: unparsed as u64,
        }
    }
}
