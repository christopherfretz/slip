//! `slip rename` (`spec/exec.md` §8.11, `spec/edits.md` §10.1): the whole
//! run of a native rename. `map`'s steps with a rename request, reduce,
//! `check`'s steps, then, when `--write` or `--patch` was given and the
//! result's state permits it, what `apply` does; one transform run record
//! for all of it. Work package WP-076.

use std::io::Write;
use std::os::unix::ffi::OsStrExt;

use slip_exec::store::{Access, Object};
use slip_model::editset::SetState;
use slip_model::transform_run::StepName;
use slip_verify::conclude::Allow;

use super::apply::{self, Applied, apply_result, refused_name};
use super::check::{CheckInput, check, status_of};
use super::lines::edit_set_summary;
use super::map::{MapInput, map, request_of, start};
use super::reduce::reduce_round;
use super::report::{Details, details};
use super::run_record::{Clock, Record, install_signals, interrupted};
use super::{CommandError, failure};
use crate::args::{ApplyAction, RenameArgs};
use crate::commands::{self, Failure, open_store};

/// Whether the command may write what was asked: never after an
/// interruption (`spec/exec.md` §7; WP-076 "Cancellation"), whatever the
/// result permits.
pub fn may_write(asked: bool, interrupted: bool) -> bool {
    asked && !interrupted
}

/// `slip rename` (§10.1, §10.5).
#[allow(
    clippy::too_many_lines,
    reason = "one sequence of steps over a dozen locals: map, reduce, check, write, the lines"
)]
pub(crate) fn main(
    a: &RenameArgs,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> commands::Result<u8> {
    let request = request_of(&a.target, &a.options)?;
    let relative_to = match &a.options.relative_to {
        None => None,
        Some(d) => Some(
            std::fs::canonicalize(d)
                .map_err(|e| Failure::Input(format!("{}: {e}", d.display())))?
                .as_os_str()
                .as_bytes()
                .to_vec(),
        ),
    };
    let mut record = Record::new();
    let store = open_store(&a.run.store, Access::Exclusive)?;
    let published = install_signals(&store, apply::remove_patch_tmp);
    if let Some(ApplyAction::Patch(file)) = &a.output {
        for p in apply::patch_leftovers(file) {
            let _ = writeln!(
                err,
                "slip: {}: the temporary of a patch by another run (killed, or still running); \
                 not removed",
                p.display()
            );
        }
    }
    let s = start(&store, &a.source, &a.run, &request, &mut record, out, err)?;
    let tokens = commands::token_pool(a.run.jobs);
    let options = commands::pool_options(&a.run, tokens.jobs(), err);
    let worker = commands::worker_command(&a.run)?;

    // Everything from the map phase on publishes the record, whatever
    // happens (§10.6).
    let run = (|| -> Result<_, CommandError> {
        let input = MapInput {
            store: &store,
            plan_id: s.plan_id,
            plan: &s.plan,
            selected: &s.selected,
            transform_id: s.transform_id,
            transform: &request,
            round: s.round,
            pool: &options,
            worker: &worker,
            tokens: &tokens,
        };
        map(&input, &mut record.steps, out, err)?;
        if interrupted() {
            return Err(CommandError::Interrupted);
        }
        let clock = Clock::start(StepName::Reduce);
        let mut held = Vec::new();
        while held.len() < tokens.jobs() {
            match tokens.try_acquire() {
                Some(t) => held.push(t),
                None => break,
            }
        }
        let threads = held.len().max(1);
        let reduced = reduce_round(
            &store,
            s.plan_id,
            &s.plan,
            s.round,
            &s.selected,
            (s.transform_id, &request),
            threads,
        );
        drop(held);
        let (set_id, set, declared) = reduced?;
        record.steps.push(clock.end(None));
        record.edits = Some(set_id);
        record.declared = declared.as_ref().map(|d| d.0);
        edit_set_summary(
            set_id,
            &set,
            declared.as_ref().map(|(id, d)| (*id, d)),
            request.indivisible,
            out,
        );
        debug_assert!(set.state == SetState::Refused || declared.is_some());
        let input = CheckInput {
            store: &store,
            plan_id: s.plan_id,
            plan: &s.plan,
            round: s.round,
            selected: &s.selected,
            partial: s.selected.len() < s.plan.observers.len(),
            transform_id: s.transform_id,
            transform: &request,
            set_id,
            set: &set,
            declared,
            options: &a.options,
            report: a.report.as_deref(),
            findings: a.findings,
            locate: a.locate,
            pool: &options,
            worker: &worker,
            tokens: &tokens,
        };
        let checked = check(&input, &mut record, out, err)?;
        let applied = match &a.output {
            Some(action) if may_write(true, interrupted()) => {
                let clock = Clock::start(StepName::Write);
                let allow = Allow {
                    residue: a.options.allow_residue,
                    errors: a.options.allow_errors,
                    partial: a.options.allow_partial,
                };
                let applied = apply_result(
                    &store,
                    checked.result_id,
                    action,
                    a.options.relative_to.as_deref(),
                    allow,
                    out,
                )?;
                record.steps.push(clock.end(None));
                Some((action, applied))
            }
            _ => None,
        };
        Ok((set_id, set, checked, applied))
    })();
    let published_record = record.publish(&store, s.plan_id, s.round, s.transform_id);
    drop(published);
    let (set_id, set, checked, applied) = run.map_err(failure)?;
    published_record.map_err(failure)?;
    if interrupted() {
        return Err(Failure::Interrupted);
    }

    let wrote = match &applied {
        None => a.output.is_none(),
        Some((_, Applied::Written(_) | Applied::Patch(_))) => true,
        Some((action, Applied::NotPermitted(state))) => {
            let asked = if **action == ApplyAction::Write { "--write" } else { "--patch" };
            let _ = writeln!(
                err,
                "slip: result {}: its state `{}` does not permit `{asked}` with the options \
                 given; nothing was written",
                checked.result_id,
                state.name()
            );
            false
        }
        Some((_, Applied::Refused(r))) => {
            let (name, _) = refused_name(r);
            let _ = write!(err, "slip: refused {name}: ");
            let _ = err.write_all(&apply::refused_detail(r));
            let _ = writeln!(err, "; nothing was written");
            false
        }
    };
    details(
        &Details {
            store: &store,
            plan: &s.plan,
            set: &set,
            set_record: &store.object_path(Object::EditSet, &set_id),
            declared: checked.declared.as_ref(),
            result: &checked.result,
            result_record: &store.object_path(Object::Result, &checked.result_id),
            overlay: checked.overlay.as_ref(),
            report: checked.report.as_ref(),
            report_file: a.report.as_deref(),
            relative_to: relative_to.as_deref(),
        },
        out,
    );
    // The plan's exclusions are its lines' (`plan`), not the result's.
    Ok(status_of(checked.result.state, wrote))
}

#[cfg(test)]
mod tests {
    use super::*;

    // WP-076 "Cancellation": nothing is written after an interruption,
    // whatever was asked.
    #[test]
    fn an_interrupted_run_writes_nothing() {
        assert!(may_write(true, false));
        assert!(!may_write(true, true));
        assert!(!may_write(false, false));
        assert!(!may_write(false, true));
    }
}
