//! The outputs of `cargo xtask perf`: the summary (`spec/harness.md` §7.2),
//! the baseline and the check (§7.3). Every number is rendered with
//! integer arithmetic except the ratios of `check`.

use super::{
    ENDS, Loaded, Measured, add, counters, end_name, measured, shard_counters, shard_statuses,
    status_name,
};
use crate::corpus::Corpus;
use slip_exec::store;
use slip_model::Digest;
use slip_model::json::{self, Value};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// How many observers each `top` list of the summary shows.
pub const TOP: usize = 10;

/// `ns` as seconds with three decimals, rounded half up (`1_234_500_000`
/// is `1.235`), computed in integers.
pub fn seconds(ns: u64) -> String {
    let ms = (u128::from(ns) + 500_000) / 1_000_000;
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// `num / den` with two decimals, rounded half up, computed in integers;
/// `-` when `den` is 0.
fn ratio2(num: u128, den: u128) -> String {
    if den == 0 {
        return "-".to_owned();
    }
    // num < 2^64, so num × 100 cannot overflow; den may be close to 2^128,
    // so the half-up test compares the remainder with den − r instead of
    // doubling either.
    let scaled = num * 100;
    let (q, r) = (scaled / den, scaled % den);
    let h = if r >= den - r { q + 1 } else { q };
    format!("{}.{:02}", h / 100, h % 100)
}

/// A path or other bytes as one line of text: valid UTF-8 as itself,
/// except that `\` is written `\\` and control characters and bytes that
/// are not UTF-8 are written `\xNN`.
pub fn text(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                c if c.is_control() && (c as u32) < 0x100 => {
                    let _ = write!(out, "\\x{:02x}", c as u32);
                }
                c => out.push(c),
            }
        }
        for b in chunk.invalid() {
            let _ = write!(out, "\\x{b:02x}");
        }
    }
    out
}

/// The summary of one record (§7.2), ending with a newline. `given` is the
/// path as given on the command line. A record with missing shards (a
/// cancelled run) is summarised from what is present, with a line
/// `missing shards <n>` after the `shard` lines. `Err` only for an
/// overflowing sum.
pub fn render_summary(given: &str, loaded: &Loaded) -> Result<String, String> {
    let rec = &loaded.record;
    let run = &rec.run;
    let mut s = String::new();
    let mut line = |l: String| {
        s.push_str(&l);
        s.push('\n');
    };
    line(format!("run {}", text(given.as_bytes())));
    line(format!("plan {}", loaded.path.plan));
    line(format!("phase {} round {}", store::phase_name(loaded.path.phase), loaded.path.round));
    line(format!("tool slip {} kiln {}", text(run.tool_slip.as_bytes()), text(run.tool_kiln.as_bytes())));
    line(format!("observers {} of {}", loaded.shards.len(), loaded.plan.observers.len()));
    for (st, n) in shard_statuses(loaded) {
        line(format!("shard {} {n}", status_name(st)));
    }
    let missing = loaded.missing_shards();
    if missing > 0 {
        line(format!("missing shards {missing}"));
    }
    for end in ENDS {
        let n = rec.jobs.iter().filter(|j| j.end == end).count();
        line(format!("job {} {n}", end_name(end)));
    }
    let m = measured(rec)?;
    line(format!("wall_s {}", seconds(m.wall_ns)));
    line(format!("cpu_s {}", seconds(m.cpu_ns)));
    let divisor = u128::from(m.wall_ns) * u128::from(m.workers_max);
    line(format!("efficiency {}", ratio2(u128::from(m.cpu_ns), divisor)));
    line(format!(
        "workers_max {} cores {} memory_budget {}",
        m.workers_max, m.cores, m.memory_budget
    ));
    line(format!(
        "peak_rss max {} p50 {} p90 {} p99 {} sum {}",
        m.peak_rss_max, m.peak_rss_p50, m.peak_rss_p90, m.peak_rss_p99, m.peak_rss_sum
    ));
    for (name, value) in shard_counters(loaded)? {
        line(format!("counter {name} {value}"));
    }
    // Per observer: summed cpu_ns and the greatest peak_rss of any job.
    let mut per: BTreeMap<Digest, (u64, u64)> = BTreeMap::new();
    for j in &rec.jobs {
        let e = per.entry(j.observer).or_insert((0, 0));
        e.0 = add(e.0, j.cpu_ns, &format!("cpu_ns summed over the jobs of observer {}", j.observer))?;
        e.1 = e.1.max(j.peak_rss);
    }
    let main_of = |id: &Digest| {
        loaded
            .plan
            .observers
            .iter()
            .find(|o| o.id == *id)
            .map_or_else(|| id.to_string(), |o| text(&o.main.path))
    };
    let mut by_cpu: Vec<(&Digest, u64)> = per.iter().map(|(id, v)| (id, v.0)).collect();
    by_cpu.sort_by_key(|&(id, v)| (Reverse(v), *id));
    line("top cpu".to_owned());
    for (id, v) in by_cpu.into_iter().take(TOP) {
        line(format!("  {} {}", seconds(v), main_of(id)));
    }
    let mut by_rss: Vec<(&Digest, u64)> = per.iter().map(|(id, v)| (id, v.1)).collect();
    by_rss.sort_by_key(|&(id, v)| (Reverse(v), *id));
    line("top peak_rss".to_owned());
    for (id, v) in by_rss.into_iter().take(TOP) {
        line(format!("  {v} {}", main_of(id)));
    }
    Ok(s)
}

// ---------------------------------------------------------------------------
// §7.3 Baselines

/// A measured value: seconds (a JSON `Float`) or a whole number of bytes
/// or units (a JSON `Int`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    Seconds(f64),
    Count(u64),
}

/// The members of `measured`, in order, and whether each is in seconds.
pub const MEASURED: [(&str, bool); 10] = [
    ("wall_s", true),
    ("cpu_s", true),
    ("peak_rss_max", false),
    ("peak_rss_p50", false),
    ("peak_rss_p90", false),
    ("peak_rss_p99", false),
    ("peak_rss_sum", false),
    ("cores", false),
    ("workers_max", false),
    ("memory_budget", false),
];

/// The seconds of `ns` as the `f64` nearest to their three-decimal text
/// ([`seconds`]): the value a baseline stores.
fn seconds_f64(ns: u64) -> f64 {
    let t = seconds(ns);
    t.parse().unwrap_or_else(|e| unreachable!("`{t}` is a decimal number: {e}"))
}

impl Measured {
    /// The values as text, in the order of [`MEASURED`]: seconds by
    /// [`seconds`], the others in decimal.
    pub fn texts(&self) -> [String; 10] {
        [
            seconds(self.wall_ns),
            seconds(self.cpu_ns),
            self.peak_rss_max.to_string(),
            self.peak_rss_p50.to_string(),
            self.peak_rss_p90.to_string(),
            self.peak_rss_p99.to_string(),
            self.peak_rss_sum.to_string(),
            self.cores.to_string(),
            self.workers_max.to_string(),
            self.memory_budget.to_string(),
        ]
    }

    /// The values in the order of [`MEASURED`].
    pub fn values(&self) -> [(&'static str, Num); 10] {
        let v = [
            Num::Seconds(seconds_f64(self.wall_ns)),
            Num::Seconds(seconds_f64(self.cpu_ns)),
            Num::Count(self.peak_rss_max),
            Num::Count(self.peak_rss_p50),
            Num::Count(self.peak_rss_p90),
            Num::Count(self.peak_rss_p99),
            Num::Count(self.peak_rss_sum),
            Num::Count(self.cores),
            Num::Count(self.workers_max),
            Num::Count(self.memory_budget),
        ];
        let mut i = 0;
        v.map(|n| {
            let name = MEASURED[i].0;
            i += 1;
            (name, n)
        })
    }
}

/// A baseline file (§7.3), as read.
#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    pub corpus: String,
    pub pin: String,
    pub slip: String,
    pub kiln: String,
    pub counters: BTreeMap<String, u64>,
    /// In the order of [`MEASURED`].
    pub measured: Vec<(String, Num)>,
}

fn int(v: u64) -> Value {
    Value::Int(i128::from(v))
}

/// The baseline for `corpus` from `loaded` (§7.3), in the pretty JSON form,
/// ending with a newline. Refused (`Err`) for a record that does not cover
/// the plan, and for anything [`counters`] refuses.
pub fn render_baseline(corpus: &Corpus, loaded: &Loaded) -> Result<String, String> {
    let (run_obs, plan_obs) = (loaded.shards.len(), loaded.plan.observers.len());
    if run_obs != plan_obs {
        return Err(format!(
            "{}: the record covers {run_obs} of the plan's {plan_obs} observers; a baseline is \
             recorded from a run over the whole plan",
            loaded.path.store.display()
        ));
    }
    let counters = counters(loaded)?;
    let m = measured(&loaded.record)?;
    let measured = m
        .values()
        .into_iter()
        .map(|(name, n)| {
            let v = match n {
                Num::Seconds(s) => Value::Float(s),
                Num::Count(c) => int(c),
            };
            (name.to_owned(), v)
        })
        .collect();
    let run = &loaded.record.run;
    let value = Value::Object(vec![
        ("corpus".to_owned(), Value::Str(corpus.name.clone())),
        ("pin".to_owned(), Value::Str(corpus.tag.clone())),
        ("slip".to_owned(), Value::Str(run.tool_slip.clone())),
        ("kiln".to_owned(), Value::Str(run.tool_kiln.clone())),
        (
            "counters".to_owned(),
            Value::Object(counters.into_iter().map(|(k, v)| (k, int(v))).collect()),
        ),
        ("measured".to_owned(), Value::Object(measured)),
    ]);
    json::write_pretty(&value).map_err(|e| format!("the baseline cannot be written as JSON: {e}"))
}

/// Reads a baseline's text. Every member is required and no other member is
/// allowed (members in any order); `counters` values and the whole-number
/// `measured` values are integers in 0..2^64, `wall_s` and `cpu_s`
/// non-negative numbers with a fraction or exponent. Errors name the JSON
/// path (`counters.inputs.files`, `measured.wall_s`).
pub fn read_baseline(bytes: &[u8]) -> Result<Baseline, String> {
    let root = json::parse(bytes).map_err(|e| format!("not JSON: {e}"))?;
    let members = root.as_object().ok_or("at the top level: expected an object")?;
    let get = |key: &str| -> Result<&Value, String> {
        members.iter().find(|(k, _)| k == key).map(|(_, v)| v).ok_or(format!("no member `{key}`"))
    };
    for (k, _) in members {
        if !["corpus", "pin", "slip", "kiln", "counters", "measured"].contains(&k.as_str()) {
            return Err(format!("at {k}: unknown member"));
        }
    }
    let string = |key: &str| -> Result<String, String> {
        get(key)?.as_str().map(str::to_owned).ok_or(format!("at {key}: expected a string"))
    };
    let (corpus, pin, slip, kiln) = (string("corpus")?, string("pin")?, string("slip")?, string("kiln")?);
    let mut counters = BTreeMap::new();
    let cs = get("counters")?.as_object().ok_or("at counters: expected an object")?;
    for (k, v) in cs {
        let n = v
            .as_u64()
            .ok_or(format!("at counters.{k}: expected an integer in 0..2^64"))?;
        counters.insert(k.clone(), n);
    }
    let ms = get("measured")?.as_object().ok_or("at measured: expected an object")?;
    for (k, _) in ms {
        if !MEASURED.iter().any(|(n, _)| n == k) {
            return Err(format!("at measured.{k}: unknown member"));
        }
    }
    let mut measured = Vec::new();
    for (name, is_seconds) in MEASURED {
        let v = ms
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
            .ok_or(format!("at measured: no member `{name}`"))?;
        let n = if is_seconds {
            match v {
                Value::Float(x) if *x >= 0.0 => Num::Seconds(*x),
                _ => return Err(format!("at measured.{name}: expected a non-negative number of seconds with a fraction")),
            }
        } else {
            Num::Count(v.as_u64().ok_or(format!("at measured.{name}: expected an integer in 0..2^64"))?)
        };
        measured.push((name.to_owned(), n));
    }
    Ok(Baseline { corpus, pin, slip, kiln, counters, measured })
}

/// For a ratio only (§7.3: ratios may use `f64`), so precision lost above
/// 2^53 does not matter.
#[allow(clippy::cast_precision_loss)]
fn as_f64(n: Num) -> f64 {
    match n {
        Num::Seconds(s) => s,
        Num::Count(c) => c as f64,
    }
}

/// Compares `loaded` with `baseline` for `corpus` (§7.3): the report and
/// whether it passed. `pin` and every counter are compared exactly and fail
/// the check; the tool versions are printed when they differ and the
/// measured values always, with their ratio to the baseline, and neither
/// fails it. `Err` for anything [`counters`] refuses.
pub fn check(baseline: &Baseline, corpus: &Corpus, loaded: &Loaded) -> Result<(String, bool), String> {
    let run_counters = counters(loaded)?;
    let m = measured(&loaded.record)?;
    let run = &loaded.record.run;
    let mut s = String::new();
    let mut failures = 0usize;
    if baseline.pin != corpus.tag {
        failures += 1;
        let _ = writeln!(s, "pin: baseline {}, run {}", text(baseline.pin.as_bytes()), text(corpus.tag.as_bytes()));
    }
    for (what, a, b) in [("slip", &baseline.slip, &run.tool_slip), ("kiln", &baseline.kiln, &run.tool_kiln)] {
        if a != b {
            let _ = writeln!(s, "{what}: baseline {}, run {}", text(a.as_bytes()), text(b.as_bytes()));
        }
    }
    let names: std::collections::BTreeSet<&String> =
        baseline.counters.keys().chain(run_counters.keys()).collect();
    for name in names {
        let (a, b) = (baseline.counters.get(name), run_counters.get(name));
        if a != b {
            failures += 1;
            let shown = |v: Option<&u64>| v.map_or_else(|| "-".to_owned(), u64::to_string);
            let _ = writeln!(s, "{}: baseline {}, run {}", text(name.as_bytes()), shown(a), shown(b));
        }
    }
    for (((name, now), shown), (_, then)) in
        m.values().into_iter().zip(m.texts()).zip(&baseline.measured)
    {
        let base = as_f64(*then);
        let ratio = if base == 0.0 { "-".to_owned() } else { format!("{:.2}", as_f64(now) / base) };
        let _ = writeln!(s, "{name} {shown} (\u{d7}{ratio})");
    }
    if failures == 0 {
        let _ = writeln!(s, "perf check: {}: ok", corpus.name);
    } else {
        let _ = writeln!(s, "perf check: {}: FAILED ({failures} difference(s))", corpus.name);
    }
    Ok((s, failures == 0))
}
