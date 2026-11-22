//! The plan, kind `PLAN` (`spec/model.md` §7).

use std::collections::HashSet;

use crate::json::Value;
use crate::wire::{Decoder, Encoder};
use crate::{Configuration, Digest, Excluded, Observer, Origin, Plan};

use super::common::{
    JResult, Result, dec_command, dec_enum, dec_file_ref, dec_list, enc_command, enc_enum,
    enc_file_ref, json_bytes, json_command, json_digest, json_enum, json_file_ref, json_uv,
    malformed, object, read_bytes, read_command, read_digest, read_enum, read_file_ref, read_list,
    read_record, read_str, read_uv,
};
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, open, read_container, render_container,
    seal,
};

const TOOL: u64 = 1;
const CONFIGURATIONS: u64 = 2;
const OBSERVERS: u64 = 3;
const EXCLUDED: u64 = 4;

// ---------------------------------------------------------------------------
// Invariants

/// Where in one observer record a rule is broken.
#[derive(Clone, Copy, Debug)]
enum ObsLoc {
    Id,
    Env(usize),
    Origins,
    Origin(usize),
}

/// Where in a plan a rule is broken.
#[derive(Clone, Copy, Debug)]
enum Loc {
    ConfigurationName(usize),
    Observer(usize),
    InObserver(usize, ObsLoc),
    Excluded(usize),
}

impl Loc {
    fn path(self) -> String {
        match self {
            Loc::ConfigurationName(i) => format!("sections.configurations[{i}].name"),
            Loc::Observer(i) => format!("sections.observers[{i}]"),
            Loc::InObserver(i, l) => {
                let o = format!("sections.observers[{i}]");
                match l {
                    ObsLoc::Id => format!("{o}.id"),
                    ObsLoc::Env(j) => format!("{o}.command.env[{j}]"),
                    ObsLoc::Origins => format!("{o}.origins"),
                    ObsLoc::Origin(j) => format!("{o}.origins[{j}]"),
                }
            }
            Loc::Excluded(i) => format!("sections.excluded[{i}]"),
        }
    }
}

/// The rules one observer record obeys on its own (§7).
fn observer_violation(o: &Observer) -> Option<(ObsLoc, &'static str)> {
    if o.id != o.command.observer_id() {
        return Some((ObsLoc::Id, "observer id is not ObserverId(command)"));
    }
    if let Some(j) =
        (1..o.command.env.len()).find(|&j| o.command.env[j - 1].name >= o.command.env[j].name)
    {
        return Some((ObsLoc::Env(j), "command env is not strictly ascending by name"));
    }
    if o.origins.is_empty() {
        return Some((ObsLoc::Origins, "observer has no origin"));
    }
    if let Some(j) = (1..o.origins.len()).find(|&j| o.origins[j - 1] >= o.origins[j]) {
        return Some((ObsLoc::Origin(j), "origins are not strictly ascending"));
    }
    None
}

/// The first violation of the plan's invariants, in section order. With
/// `records` false the rules of [`observer_violation`] are taken as
/// already checked.
fn violation(p: &Plan, records: bool) -> Option<(Loc, &'static str)> {
    let mut names = HashSet::new();
    for (i, c) in p.configurations.iter().enumerate() {
        if !names.insert(c.name.as_str()) {
            return Some((Loc::ConfigurationName(i), "configuration name is not unique"));
        }
    }
    let configurations = p.configurations.len() as u64;
    let mut ids: HashSet<Digest> = HashSet::new();
    let mut pairs: HashSet<Origin> = HashSet::new();
    for (i, o) in p.observers.iter().enumerate() {
        if records && let Some((l, what)) = observer_violation(o) {
            return Some((Loc::InObserver(i, l), what));
        }
        if let Some(prev) = i.checked_sub(1).map(|k| &p.observers[k]) {
            let ordered =
                o.cost_hint < prev.cost_hint || (o.cost_hint == prev.cost_hint && o.id > prev.id);
            if !ordered {
                return Some((Loc::Observer(i), "observers are not in plan order"));
            }
        }
        if !ids.insert(o.id) {
            return Some((Loc::InObserver(i, ObsLoc::Id), "observer id is not unique"));
        }
        for (j, origin) in o.origins.iter().enumerate() {
            let at = Loc::InObserver(i, ObsLoc::Origin(j));
            if origin.configuration >= configurations {
                return Some((at, "configuration index out of range"));
            }
            if !pairs.insert(*origin) {
                return Some((at, "(configuration, entry) pair appears twice"));
            }
        }
    }
    for (k, x) in p.excluded.iter().enumerate() {
        let at = Loc::Excluded(k);
        let pair = Origin { configuration: x.configuration, entry: x.entry };
        if let Some(prev) = k.checked_sub(1).map(|k| &p.excluded[k])
            && (prev.configuration, prev.entry) >= (x.configuration, x.entry)
        {
            return Some((at, "excluded is not strictly ascending by (configuration, entry)"));
        }
        if x.configuration >= configurations {
            return Some((at, "configuration index out of range"));
        }
        if !pairs.insert(pair) {
            return Some((at, "(configuration, entry) pair appears twice"));
        }
    }
    None
}

pub(super) fn check(p: &Plan) -> std::result::Result<(), (String, &'static str)> {
    violation(p, true).map_or(Ok(()), |(l, what)| Err((l.path(), what)))
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn enc_observer(e: &mut Encoder, o: &Observer) {
    e.digest(&o.id);
    enc_command(e, &o.command);
    enc_enum(e, o.language);
    enc_file_ref(e, &o.main);
    e.uv(o.cost_hint).count(o.origins.len());
    for g in &o.origins {
        e.uv(g.configuration).uv(g.entry);
    }
}

/// One observer record, validated on its own ([`observer_violation`]). The
/// offset of each origin is appended to `origin_offsets`.
pub(super) fn dec_observer(
    d: &mut Decoder<'_>,
    origin_offsets: &mut Vec<usize>,
) -> Result<Observer> {
    let start = d.offset();
    let id = d.digest()?;
    let mut env_offsets = Vec::new();
    let command = dec_command(d, Some(&mut env_offsets))?;
    let language = dec_enum(d)?;
    let main = dec_file_ref(d)?;
    let cost_hint = d.uv()?;
    let origins_at = d.offset();
    let base = origin_offsets.len();
    let origins = dec_list(d, Some(origin_offsets), |d| {
        Ok(Origin { configuration: d.uv()?, entry: d.uv()? })
    })?;
    let o = Observer { id, command, language, main, cost_hint, origins };
    if let Some((l, what)) = observer_violation(&o) {
        let at = match l {
            ObsLoc::Id => start,
            ObsLoc::Env(j) => env_offsets[j],
            ObsLoc::Origins => origins_at,
            ObsLoc::Origin(j) => origin_offsets[base + j],
        };
        return Err(malformed(at, what));
    }
    Ok(o)
}

pub(super) fn encode(p: &Plan, extras: &[Extra<'_>]) -> Vec<u8> {
    let mut tool = Encoder::new();
    tool.str(&p.tool_slip).str(&p.tool_kiln);
    let mut configurations = Encoder::new();
    configurations.count(p.configurations.len());
    for c in &p.configurations {
        configurations.str(&c.name);
        enc_file_ref(&mut configurations, &c.database);
    }
    let mut observers = Encoder::new();
    observers.count(p.observers.len());
    for o in &p.observers {
        enc_observer(&mut observers, o);
    }
    let mut excluded = Encoder::new();
    excluded.count(p.excluded.len());
    for x in &p.excluded {
        excluded.uv(x.configuration).uv(x.entry).bytes(&x.file);
        enc_enum(&mut excluded, x.reason);
        excluded.str(&x.detail);
    }
    seal(
        Plan::KIND,
        Plan::VERSION,
        &[
            (TOOL, tool.as_bytes()),
            (CONFIGURATIONS, configurations.as_bytes()),
            (OBSERVERS, observers.as_bytes()),
            (EXCLUDED, excluded.as_bytes()),
        ],
        extras,
    )
}

/// The plan, its unknown ignorable sections and its content id.
pub(super) fn decode(bytes: &[u8]) -> Result<(Plan, Vec<Extra<'_>>, Digest)> {
    let opened =
        open(bytes, Plan::KIND, Plan::VERSION, [TOOL, CONFIGURATIONS, OBSERVERS, EXCLUDED])?;
    let end = opened.end;
    let [tool, configurations, observers, excluded] = opened.sections;

    let mut d = super::required(tool, end, "plan has no tool section")?;
    let tool_slip = d.str()?.to_owned();
    let tool_kiln = d.str()?.to_owned();
    d.finish()?;

    let mut d = super::required(configurations, end, "plan has no configurations section")?;
    let mut configuration_offsets = Vec::new();
    let configurations = dec_list(&mut d, Some(&mut configuration_offsets), |d| {
        Ok(Configuration { name: d.str()?.to_owned(), database: dec_file_ref(d)? })
    })?;
    d.finish()?;

    let mut d = super::required(observers, end, "plan has no observers section")?;
    let mut observer_offsets = Vec::new();
    let mut origin_offsets = Vec::new();
    let observers =
        dec_list(&mut d, Some(&mut observer_offsets), |d| dec_observer(d, &mut origin_offsets))?;
    d.finish()?;

    let mut d = super::required(excluded, end, "plan has no excluded section")?;
    let mut excluded_offsets = Vec::new();
    let excluded = dec_list(&mut d, Some(&mut excluded_offsets), |d| {
        Ok(Excluded {
            configuration: d.uv()?,
            entry: d.uv()?,
            file: d.bytes()?.to_vec(),
            reason: dec_enum(d)?,
            detail: d.str()?.to_owned(),
        })
    })?;
    d.finish()?;

    let plan = Plan { tool_slip, tool_kiln, configurations, observers, excluded };
    if let Some((l, what)) = violation(&plan, false) {
        let at = match l {
            Loc::ConfigurationName(i) => configuration_offsets[i],
            Loc::InObserver(i, ObsLoc::Origin(j)) => {
                let before: usize = plan.observers[..i].iter().map(|o| o.origins.len()).sum();
                origin_offsets[before + j]
            }
            Loc::Observer(i) | Loc::InObserver(i, _) => observer_offsets[i],
            Loc::Excluded(k) => excluded_offsets[k],
        };
        return Err(malformed(at, what));
    }
    Ok((plan, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 4] = [
    SectionName { id: TOOL, name: "tool", required: true },
    SectionName { id: CONFIGURATIONS, name: "configurations", required: true },
    SectionName { id: OBSERVERS, name: "observers", required: true },
    SectionName { id: EXCLUDED, name: "excluded", required: true },
];

fn json_observer(o: &Observer) -> Value {
    object(vec![
        ("id", json_digest(&o.id)),
        ("command", json_command(&o.command)),
        ("language", json_enum(o.language)),
        ("main", json_file_ref(&o.main)),
        ("cost_hint", json_uv(o.cost_hint)),
        (
            "origins",
            Value::Array(
                o.origins
                    .iter()
                    .map(|g| {
                        object(vec![
                            ("configuration", json_uv(g.configuration)),
                            ("entry", json_uv(g.entry)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

pub(super) fn render(p: &Plan, extras: &[Extra<'_>]) -> Value {
    let tool = object(vec![
        ("slip", Value::Str(p.tool_slip.clone())),
        ("kiln", Value::Str(p.tool_kiln.clone())),
    ]);
    let configurations = p
        .configurations
        .iter()
        .map(|c| {
            object(vec![
                ("name", Value::Str(c.name.clone())),
                ("database", json_file_ref(&c.database)),
            ])
        })
        .collect();
    let excluded = p
        .excluded
        .iter()
        .map(|x| {
            object(vec![
                ("configuration", json_uv(x.configuration)),
                ("entry", json_uv(x.entry)),
                ("file", json_bytes(&x.file)),
                ("reason", json_enum(x.reason)),
                ("detail", Value::Str(x.detail.clone())),
            ])
        })
        .collect();
    render_container(
        Plan::KIND,
        Plan::VERSION,
        vec![
            ("tool", tool),
            ("configurations", Value::Array(configurations)),
            ("observers", Value::Array(p.observers.iter().map(json_observer).collect())),
            ("excluded", Value::Array(excluded)),
        ],
        extras,
    )
}

fn read_observer(v: &Value, at: &At<'_>) -> JResult<Observer> {
    let [id, command, language, main, cost_hint, origins] =
        read_record(v, at, ["id", "command", "language", "main", "cost_hint", "origins"])?;
    Ok(Observer {
        id: read_digest(id, &at.key("id"))?,
        command: read_command(command, &at.key("command"))?,
        language: read_enum(language, &at.key("language"))?,
        main: read_file_ref(main, &at.key("main"))?,
        cost_hint: read_uv(cost_hint, &at.key("cost_hint"))?,
        origins: read_list(origins, &at.key("origins"), |v, at| {
            let [c, e] = read_record(v, at, ["configuration", "entry"])?;
            Ok(Origin {
                configuration: read_uv(c, &at.key("configuration"))?,
                entry: read_uv(e, &at.key("entry"))?,
            })
        })?,
    })
}

/// The plan and the unknown ignorable sections of a JSON rendering.
pub(super) fn read(v: &Value) -> JResult<(Plan, OwnedExtras)> {
    let s = read_container(v, Plan::KIND, Plan::VERSION, &NAMES)?;
    let [Some(tool), Some(configurations), Some(observers), Some(excluded)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");

    let at = sections.key("tool");
    let [slip, kiln] = read_record(tool, &at, ["slip", "kiln"])?;
    let tool_slip = read_str(slip, &at.key("slip"))?.to_owned();
    let tool_kiln = read_str(kiln, &at.key("kiln"))?.to_owned();

    let configurations = read_list(configurations, &sections.key("configurations"), |v, at| {
        let [name, database] = read_record(v, at, ["name", "database"])?;
        Ok(Configuration {
            name: read_str(name, &at.key("name"))?.to_owned(),
            database: read_file_ref(database, &at.key("database"))?,
        })
    })?;
    let observers = read_list(observers, &sections.key("observers"), read_observer)?;
    let excluded = read_list(excluded, &sections.key("excluded"), |v, at| {
        let [configuration, entry, file, reason, detail] =
            read_record(v, at, ["configuration", "entry", "file", "reason", "detail"])?;
        Ok(Excluded {
            configuration: read_uv(configuration, &at.key("configuration"))?,
            entry: read_uv(entry, &at.key("entry"))?,
            file: read_bytes(file, &at.key("file"))?,
            reason: read_enum(reason, &at.key("reason"))?,
            detail: read_str(detail, &at.key("detail"))?.to_owned(),
        })
    })?;

    let plan = Plan { tool_slip, tool_kiln, configurations, observers, excluded };
    if let Some((l, what)) = violation(&plan, true) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((plan, s.extras))
}
