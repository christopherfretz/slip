//! The five oracles of `spec/harness.md` §6.4, in the order they run:
//! `clang` on the written tree, `back` on it, then (the tree restored to
//! `pre/`) `verify` and `mutation` in the run's store, and `determinism` in
//! a fresh store last. Each failure is one message; a run killed by a
//! signal or its timeout, or one the runner cannot judge, ends the test at
//! once. An oracle that does not run, for any reason but `oracle:
//! no-<name>`, is a skip with its reason in the test's [`Report`].

use super::mutate::{self, Kind, Mutation};
use super::run::{
    Bounded, Ctx, Ran, Record, check_output, cmdline, oracle_store, read_object, read_result,
    run_bounded,
};
use super::test_file::Oracle;
use super::tree::{self, Files};
use super::{Note, Outcome, Report};
use slip_exec::store::{Access, Object, Store};
use slip_model::edits::Piece;
use slip_model::editset::{SetState, within};
use slip_model::overlay::Overlay;
use slip_model::result::{Category, State};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn skip(report: &mut Report, oracle: Oracle, reason: impl Into<String>) {
    report.notes.push(Note { oracle, off: false, reason: reason.into() });
}

/// Runs the oracles after a run `ran` whose goldens passed; `got` is the
/// tree it wrote, `record` its records (none for a run without a result).
/// Returns the failures; records the skips in `report`.
pub fn run_all(
    ctx: &Ctx<'_>,
    ran: &Ran,
    record: Option<&Record>,
    got: &Files,
    clang: Option<&Path>,
    report: &mut Report,
) -> Result<Vec<String>, Outcome> {
    let test = ctx.test;
    let written = *got != ctx.pre;
    let mut failures = Vec::new();
    let not_written = "the tree was not written";

    if test.oracle_on(Oracle::Clang) {
        match (written, clang) {
            (false, _) => skip(report, Oracle::Clang, not_written),
            (true, None) => {
                report.clang_skipped = true;
                skip(report, Oracle::Clang, "no compiler (none found, or --no-clang)");
            }
            (true, Some(c)) => failures.extend(clang_written(ctx, c, got)),
        }
    }
    if test.oracle_on(Oracle::Back) {
        if written {
            failures.extend(back(ctx, ran)?);
        } else {
            skip(report, Oracle::Back, not_written);
        }
    }
    ctx.restore_tree().map_err(Outcome::Error)?;
    let verified =
        record.filter(|r| matches!(r.result.state, State::Verified | State::VerifiedWithResidue));
    let why = record.map_or("the run has no result".to_owned(), |r| {
        format!("the result is {}", r.result.state.name())
    });
    if test.oracle_on(Oracle::Verify) {
        match verified {
            Some(r) => failures.extend(verify(ctx, r)?),
            None => skip(report, Oracle::Verify, why.clone()),
        }
    }
    if test.oracle_on(Oracle::Mutation) {
        match verified {
            Some(r) => failures.extend(mutation(ctx, r)?),
            None => skip(report, Oracle::Mutation, why),
        }
    }
    if test.oracle_on(Oracle::Determinism) {
        failures.extend(determinism(ctx, ran, got)?);
    }
    Ok(failures)
}

/// `clang` on the written tree: every command accepts it, and every file
/// the run changed was read by one of them (else the oracle compiled
/// nothing that changed).
fn clang_written(ctx: &Ctx<'_>, clang: &Path, got: &Files) -> Vec<String> {
    let deps = ctx.work.join("clang");
    let read = match self::clang(ctx, clang, Some(&deps)) {
        Ok(read) => read,
        Err(e) => return vec![format!("clang: the written tree is not accepted: {e}")],
    };
    tree::changed_files(&ctx.pre, got)
        .into_iter()
        .filter(|p| !read.contains(&ctx.tree.join(p)))
        .map(|p| format!("clang: post-image not compiled: {p}"))
        .collect()
}

/// `clang`: every command of the database, with its compiler replaced by
/// `clang` and `-fsyntax-only` added, run in its directory, exits 0.
/// `Err` names the first that does not. With `deps`, each also writes the
/// files it read (`-MD -MF <deps>/<n>.d -MT dep`), and the paths it read
/// are returned, resolved against the command's directory.
pub fn clang(ctx: &Ctx<'_>, clang: &Path, deps: Option<&Path>) -> Result<Vec<PathBuf>, String> {
    let mut read = Vec::new();
    if let Some(d) = deps {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    for (i, (dir, argv)) in ctx.commands.iter().enumerate() {
        let mut cmd = Command::new(clang);
        cmd.args(&argv[1..]).arg("-fsyntax-only").current_dir(dir);
        let depfile = deps.map(|d| d.join(format!("{i}.d")));
        if let Some(f) = &depfile {
            cmd.arg("-MD").arg("-MF").arg(f).arg("-MT").arg("dep");
        }
        let shown = format!("`{} {} -fsyntax-only`", clang.display(), argv[1..].join(" "));
        let out = match run_bounded(cmd, ctx.timeout()) {
            Ok(o) => o,
            Err(Bounded::Spawn(e)) => return Err(format!("cannot run `{}`: {e}", clang.display())),
            Err(Bounded::TimedOut) => {
                return Err(format!("{shown} did not end within {} s", ctx.test.timeout));
            }
        };
        if !out.status.success() {
            let what = out.status.code().map_or("killed".to_owned(), |c| format!("exit {c}"));
            return Err(format!(
                "{shown} in {}: {what}: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim_end()
            ));
        }
        if let Some(f) = &depfile {
            let text = std::fs::read_to_string(f)
                .map_err(|e| format!("{shown} wrote no dependency file {}: {e}", f.display()))?;
            for p in parse_depfile(&text) {
                let p = dir.join(p);
                read.push(p.canonicalize().unwrap_or(p));
            }
        }
    }
    Ok(read)
}

/// The prerequisites of a make rule as clang's `-MD` writes it:
/// `<target>: <file> <file> \` continued over lines, a space in a name as
/// `\ ` and `$` as `$$`.
pub fn parse_depfile(text: &str) -> Vec<String> {
    let joined = text.replace("\\\n", " ");
    let body = joined.split_once(": ").map_or("", |(_, b)| b);
    let mut out = Vec::new();
    let mut word = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&' ') => {
                word.push(' ');
                chars.next();
            }
            '$' if chars.peek() == Some(&'$') => {
                word.push('$');
                chars.next();
            }
            ' ' | '\t' | '\n' | '\r' => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// `back`: the inverse transform on the written tree, with a fresh store,
/// ends in the same state and leaves the tree byte-identical to `pre/`.
fn back(ctx: &Ctx<'_>, ran: &Ran) -> Result<Vec<String>, Outcome> {
    let Some(inverse) = cmdline::inverse(&ctx.test.run) else {
        return Err(Outcome::Error(format!(
            "the back oracle has no inverse of `{}` (turn it off with `oracle: no-back <reason>`)",
            ctx.test.run.join(" ")
        )));
    };
    let store = oracle_store(ctx, "back")?;
    let args = cmdline::transform(ctx, &inverse, &store, 2);
    let back = ctx.slip(&args)?;
    let mut f = Vec::new();
    if back.lines.state != ran.lines.state {
        f.push(format!(
            "back: `{}` ends in state {}, the run in {}",
            inverse.join(" "),
            back.lines.state.as_deref().unwrap_or("none"),
            ran.lines.state.as_deref().unwrap_or("none")
        ));
    }
    let got = ctx.read_tree().map_err(Outcome::Error)?;
    if let Some(d) = tree::first_difference(&ctx.pre, &got) {
        f.push(format!("back: the tree is not pre/ again: {d}"));
    }
    Ok(f)
}

/// `verify`: `slip check` over the run's own edit set, in the same store,
/// gives the same result: the id it prints is the run's, and the record of
/// that id is the run's record, byte for byte, with the printed state.
fn verify(ctx: &Ctx<'_>, record: &Record) -> Result<Vec<String>, Outcome> {
    let r = &record.result;
    let args = cmdline::check(
        ctx,
        &ctx.store,
        &r.plan.to_string(),
        &r.round.to_string(),
        &OsString::from(r.edits.to_string()),
        None,
    );
    let check = ctx.slip(&args)?;
    if let Err(e) = check_output(&check.stdout, true) {
        return Ok(vec![format!("verify: slip check's output: {e}")]);
    }
    let Some(id) = &check.lines.result else {
        return Ok(vec![format!(
            "verify: slip check printed no result line (exit {}): {}",
            check.status,
            check.stderr.trim_end()
        )]);
    };
    if *id != record.id.to_string() {
        return Ok(vec![format!("verify: slip check gives result {id}, the run {}", record.id)]);
    }
    let bytes = match read_result(&ctx.store, id) {
        Ok((b, _)) => b,
        Err(e) => return Ok(vec![format!("verify: {e}")]),
    };
    if bytes != record.bytes {
        return Ok(vec![format!("verify: the record of result {id} is not the run's")]);
    }
    if check.lines.state.as_deref() != Some(r.state.name()) {
        return Ok(vec![format!(
            "verify: slip check printed state {}, its result record says {}",
            check.lines.state.as_deref().unwrap_or("none"),
            r.state.name()
        )]);
    }
    Ok(Vec::new())
}

/// `mutation`: each mutated edit set, published in the run's store and
/// given to `slip check` with the original's declared changes, does not
/// verify. Runs for a `verified` or `verified_with_residue` result only,
/// whose edit set is accepted (the record says so, `run::read_record`).
fn mutation(ctx: &Ctx<'_>, record: &Record) -> Result<Vec<String>, Outcome> {
    let r = &record.result;
    let Some(declared) = r.declared else {
        return Ok(vec!["mutation: the result names no declared changes".into()]);
    };
    if record.set.state != SetState::Accepted {
        return Ok(vec!["mutation: the result's edit set is not accepted".into()]);
    }
    let store = Store::open(&ctx.store, Access::Shared)
        .map_err(|e| Outcome::Error(format!("mutation: the store: {e}")))?;
    let (plan, round, declared) = (r.plan.to_string(), r.round.to_string(), declared.to_string());
    let mut f = Vec::new();
    for m in mutate::all(&record.set) {
        if let Err(e) = m.set.validate() {
            return Err(Outcome::Error(format!(
                "mutation `{}`: the mutated set is not a valid EDTS: {e}",
                m.label
            )));
        }
        let id = store
            .publish_object(Object::EditSet, &m.set.to_bytes())
            .map_err(|e| Outcome::Error(format!("mutation `{}`: publishing: {e}", m.label)))?;
        let path = store.object_path(Object::EditSet, &id);
        let args =
            cmdline::check(ctx, &ctx.store, &plan, &round, &path.into_os_string(), Some(&declared));
        let check = ctx.slip(&args)?;
        if let Err(e) = judge_mutation(&m, &check, &ctx.store) {
            f.push(format!("mutation `{}`: {e}", m.label));
        }
    }
    Ok(f)
}

/// §6.4 "Mutation": the outcome must be `not_verified` (for a changed key:
/// with a failed spelling obligation whose key is the changed key, in the
/// result's overlay), or, for a dropped edit only, `verified_with_residue`
/// with a residue entry that is not a mention, in the dropped edit's file,
/// within the dropped range. A dropped edit that leaves no edit makes a
/// refused set, which must end `refused`. The state is the result
/// record's, which must agree with the printed `state` line.
pub fn judge_mutation(m: &Mutation, check: &Ran, store: &Path) -> Result<(), String> {
    check_output(&check.stdout, true).map_err(|e| format!("slip check's output: {e}"))?;
    let Some(id) = &check.lines.result else {
        return Err(format!(
            "slip check printed no result line (exit {}): {}",
            check.status,
            check.stderr.trim_end()
        ));
    };
    let (_, result) = read_result(store, id)?;
    let state = result.state.name();
    if check.lines.state.as_deref() != Some(state) {
        return Err(format!(
            "slip check printed state {}, its result record says {state}",
            check.lines.state.as_deref().unwrap_or("none")
        ));
    }
    let refused_set = m.set.state == SetState::Refused;
    let ok = match (&m.kind, result.state) {
        (_, State::Verified) => {
            return Err("verified: an edit was lost or invented and nothing noticed".into());
        }
        (Kind::Dropped { .. }, State::Refused) if refused_set => true,
        (_, _) if refused_set => false,
        (Kind::Dropped { .. } | Kind::Added { .. }, State::NotVerified) => true,
        (Kind::Key { edit, piece }, State::NotVerified) => {
            let Some(Piece::Spelled { key, .. }) =
                m.set.edits.get(*edit).and_then(|e| e.pieces.get(*piece))
            else {
                return Err("the mutated piece is not a spelled piece".into());
            };
            let Some(o) = result.overlay else {
                return Err("not_verified, but the result names no overlay".into());
            };
            let (_, overlay) =
                read_object(store, "overlay", ("overlays", "ovly"), &o, Overlay::from_bytes)?;
            let mine: Vec<u64> = (0u64..)
                .zip(&overlay.spellings)
                .filter(|(_, ob)| ob.key == *key)
                .map(|(i, _)| i)
                .collect();
            if mine.is_empty() {
                return Err(
                    "not_verified, but the overlay has no obligation of the changed key".into()
                );
            }
            if !result.spellings.failed.iter().any(|f| mine.contains(&f.obligation)) {
                return Err(
                    "not_verified, but by no failed spelling obligation of the changed key".into(),
                );
            }
            true
        }
        (Kind::Dropped { file, lo, hi, .. }, State::VerifiedWithResidue) => {
            let found = result.residue.iter().any(|r| {
                r.category != Category::Mention
                    && r.file == *file
                    && within((r.lo, r.hi), (*lo, *hi))
            });
            if !found {
                return Err(format!(
                    "verified_with_residue without the dropped range {}:{lo}..{hi} in the residue",
                    String::from_utf8_lossy(file)
                ));
            }
            true
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        let expected = match (&m.kind, refused_set) {
            (_, true) => "refused",
            (Kind::Dropped { .. }, false) => "not_verified or verified_with_residue",
            _ => "not_verified",
        };
        Err(format!("{state}, expected {expected}"))
    }
}

/// `determinism`: a second run in a fresh store with `-j 1`, on the tree
/// restored to `pre/`, gives the same exit status, ids, standard output
/// (each store's path made `@STORE@`) and written tree.
fn determinism(ctx: &Ctx<'_>, ran: &Ran, got: &Files) -> Result<Vec<String>, Outcome> {
    let store = oracle_store(ctx, "determinism")?;
    let args = cmdline::transform(ctx, &ctx.test.run, &store, 1);
    let again = ctx.slip(&args)?;
    let mut f = Vec::new();
    if again.status != ran.status {
        f.push(format!(
            "determinism: exit status {} with -j 1, {} with -j 2",
            again.status, ran.status
        ));
    }
    for ((name, a), (_, b)) in again.lines.ids().iter().zip(ran.lines.ids().iter()) {
        if a != b {
            let show = |x: &Option<String>| x.clone().unwrap_or_else(|| "none".into());
            f.push(format!(
                "determinism: the {name} id is {} with -j 1, {} with -j 2",
                show(a),
                show(b)
            ));
        }
    }
    let first = keep_ids(&ran.stdout, &ctx.store);
    let second = keep_ids(&again.stdout, &store);
    if let Some(d) = super::run::first_line_difference(&first, &second) {
        f.push(format!("determinism: with -j 1, {d}"));
    }
    let tree = ctx.read_tree().map_err(Outcome::Error)?;
    if let Some(d) = tree::first_difference(got, &tree) {
        f.push(format!("determinism: the tree written with -j 1 is not the first run's: {d}"));
    }
    Ok(f)
}

/// The output with only the store's path replaced: the two runs share the
/// tree, and their ids must be equal.
fn keep_ids(stdout: &str, store: &Path) -> String {
    let s = store.to_string_lossy();
    if s.is_empty() { stdout.to_owned() } else { stdout.replace(s.as_ref(), "@STORE@") }
}
