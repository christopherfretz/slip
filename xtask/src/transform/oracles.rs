//! The five oracles of `spec/harness.md` §6.4, in the order they run:
//! `clang` on the written tree, `back` on it, then (the tree restored to
//! `pre/`) `verify` and `mutation` in the run's store, and `determinism` in
//! a fresh store last. Each failure is one message; a run killed by a
//! signal, or one the runner cannot judge, ends the test at once.

use super::Outcome;
use super::mutate::{self, Kind, Mutation};
use super::run::{Ctx, Ran, cmdline, oracle_store};
use super::test_file::{Expected, Oracle};
use super::tree;
use slip_exec::store::{Access, Object, Store};
use slip_model::Digest;
use slip_model::editset::{EditSet, SetState, within};
use slip_model::result::{Category, State, TransformResult};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

/// Runs the oracles after a run `ran` whose goldens passed; `written` is
/// whether the tree differs from `pre/`. Returns the failures.
pub fn run_all(
    ctx: &Ctx<'_>,
    ran: &Ran,
    written: bool,
    clang: Option<&Path>,
    clang_skipped: &mut bool,
) -> Result<Vec<String>, Outcome> {
    let test = ctx.test;
    let mut failures = Vec::new();

    if written && test.oracle_on(Oracle::Clang) {
        match clang {
            Some(c) => {
                if let Err(e) = self::clang(ctx, c) {
                    failures.push(format!("clang: the written tree is not accepted: {e}"));
                }
            }
            None => *clang_skipped = true,
        }
    }
    if written && test.oracle_on(Oracle::Back) {
        failures.extend(back(ctx, ran)?);
    }
    ctx.restore_tree().map_err(Outcome::Error)?;
    let verified = matches!(test.state, Expected::Verified | Expected::VerifiedWithResidue);
    if verified && test.oracle_on(Oracle::Verify) {
        failures.extend(verify(ctx, ran)?);
    }
    if test.oracle_on(Oracle::Mutation) {
        failures.extend(mutation(ctx, ran)?);
    }
    if test.oracle_on(Oracle::Determinism) {
        failures.extend(determinism(ctx, ran)?);
    }
    Ok(failures)
}

/// `clang`: every command of the database, with its compiler replaced by
/// `clang` and `-fsyntax-only` added, run in its directory, exits 0.
/// `Err` names the first that does not.
pub fn clang(ctx: &Ctx<'_>, clang: &Path) -> Result<(), String> {
    for (dir, argv) in &ctx.commands {
        let mut cmd = Command::new(clang);
        cmd.args(&argv[1..]).arg("-fsyntax-only").current_dir(dir).stdin(Stdio::null());
        let out = cmd.output().map_err(|e| format!("cannot run `{}`: {e}", clang.display()))?;
        if !out.status.success() {
            let what = out.status.code().map_or("killed".to_owned(), |c| format!("exit {c}"));
            return Err(format!(
                "`{} {} -fsyntax-only` in {}: {what}: {}",
                clang.display(),
                argv[1..].join(" "),
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim_end()
            ));
        }
    }
    Ok(())
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

/// The ids `check` needs from the run: plan, round, edit set.
fn check_inputs<'a>(ran: &'a Ran, oracle: &str) -> Result<(&'a str, &'a str, &'a str), String> {
    let l = &ran.lines;
    match (&l.plan, &l.round, &l.edits) {
        (Some(p), Some(r), Some(e)) => Ok((p, r, e)),
        _ => Err(format!("{oracle}: the run printed no plan, round or edits line")),
    }
}

/// `verify`: `slip check` over the run's own edit set, in the same store,
/// gives the same result id.
fn verify(ctx: &Ctx<'_>, ran: &Ran) -> Result<Vec<String>, Outcome> {
    let (plan, round, edits) = match check_inputs(ran, "verify") {
        Ok(x) => x,
        Err(e) => return Ok(vec![e]),
    };
    let args = cmdline::check(ctx, &ctx.store, plan, round, &OsString::from(edits), None);
    let check = ctx.slip(&args)?;
    Ok(match (&check.lines.result, &ran.lines.result) {
        (Some(a), Some(b)) if a == b => Vec::new(),
        (Some(a), b) => vec![format!(
            "verify: slip check gives result {a}, the run {}",
            b.as_deref().unwrap_or("none")
        )],
        (None, _) => vec![format!(
            "verify: slip check printed no result line (exit {}): {}",
            check.status,
            check.stderr.trim_end()
        )],
    })
}

/// `mutation`: each mutated edit set, published in the run's store and
/// given to `slip check` with the original's declared changes, does not
/// verify.
fn mutation(ctx: &Ctx<'_>, ran: &Ran) -> Result<Vec<String>, Outcome> {
    let Some(edits) = &ran.lines.edits else { return Ok(Vec::new()) };
    let store = Store::open(&ctx.store, Access::Shared)
        .map_err(|e| Outcome::Error(format!("mutation: the store: {e}")))?;
    let set = match read_set(&store, edits) {
        Ok(s) => s,
        Err(e) => return Ok(vec![format!("mutation: {e}")]),
    };
    if set.state != SetState::Accepted {
        return Ok(Vec::new());
    }
    let (plan, round, _) = match check_inputs(ran, "mutation") {
        Ok(x) => x,
        Err(e) => return Ok(vec![e]),
    };
    let Some(declared) = &ran.lines.declared else {
        return Ok(vec!["mutation: the run printed no declared line".into()]);
    };
    let mut f = Vec::new();
    for m in mutate::all(&set) {
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
            cmdline::check(ctx, &ctx.store, plan, round, &path.into_os_string(), Some(declared));
        let check = ctx.slip(&args)?;
        if let Err(e) = judge_mutation(&m, &check, &store) {
            f.push(format!("mutation `{}`: {e}", m.label));
        }
    }
    Ok(f)
}

fn read_set(store: &Store, id: &str) -> Result<EditSet, String> {
    let digest = Digest::from_hex(id).ok_or(format!("the edit set id `{id}` is not 64 digits"))?;
    let bytes = store
        .read_object(Object::EditSet, &digest)
        .map_err(|e| e.to_string())?
        .ok_or(format!("the store has no edit set {id}"))?;
    EditSet::from_bytes(&bytes).map_err(|e| format!("the edit set {id}: {e}"))
}

fn state_name(s: State) -> &'static str {
    match s {
        State::Verified => "verified",
        State::VerifiedWithResidue => "verified_with_residue",
        State::NotVerified => "not_verified",
        State::Incomplete => "incomplete",
        State::Refused => "refused",
    }
}

/// §6.4 "Mutation": the outcome must be `not_verified` (for a changed key:
/// with a failed spelling obligation), or, for a dropped edit only,
/// `verified_with_residue` with a residue entry that is not a mention
/// within the dropped range. A dropped edit that leaves no edit makes a
/// refused set, which must end `refused`. The state is the result
/// record's, which must agree with the last `state` line.
pub fn judge_mutation(m: &Mutation, check: &Ran, store: &Store) -> Result<(), String> {
    let Some(id) = &check.lines.result else {
        return Err(format!(
            "slip check printed no result line (exit {}): {}",
            check.status,
            check.stderr.trim_end()
        ));
    };
    let digest = Digest::from_hex(id).ok_or(format!("the result id `{id}` is not 64 digits"))?;
    let bytes = store
        .read_object(Object::Result, &digest)
        .map_err(|e| e.to_string())?
        .ok_or(format!("the store has no result {id}"))?;
    let result =
        TransformResult::from_bytes(&bytes).map_err(|e| format!("the result {id}: {e}"))?;
    let state = state_name(result.state);
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
        (Kind::Key { .. }, State::NotVerified) => {
            if result.spellings.failed.is_empty() {
                return Err("not_verified, but by no failed spelling obligation".into());
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

/// `determinism`: a second run in a fresh store with `-j 1` gives the same
/// exit status, ids and standard output (each store's path made
/// `@STORE@`).
fn determinism(ctx: &Ctx<'_>, ran: &Ran) -> Result<Vec<String>, Outcome> {
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
    Ok(f)
}

/// The output with only the store's path replaced: the two runs share the
/// tree, and their ids must be equal.
fn keep_ids(stdout: &str, store: &Path) -> String {
    let s = store.to_string_lossy();
    if s.is_empty() { stdout.to_owned() } else { stdout.replace(s.as_ref(), "@STORE@") }
}
