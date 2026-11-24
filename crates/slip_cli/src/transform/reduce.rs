//! `slip reduce` (`spec/exec.md` §8.12, `spec/edits.md` §10.2): the map
//! shards of a transform's round to the edit set and its declared
//! changes, published. It parses nothing and starts no worker.

use std::io::Write;
use std::path::PathBuf;

use slip_exec::signals;
use slip_exec::store::{self as exec_store, Access, Object, Store};
use slip_model::declared::Declared;
use slip_model::editset::{EditSet, SetState};
use slip_model::transform::Transform;
use slip_model::{Digest, Plan};
use slip_reduce::{Options, ReduceError};

use super::lines::{edit_set_details, edit_set_summary};
use super::store::{DiskFiles, StoreShards, request_of_round, selected_of_round};
use super::{CommandError, failure, store_error};
use crate::args::{DEFAULT_STORE, PlanSource, ReduceArgs};
use crate::commands::{self, Failure, load_plan, open_store, token_pool};
use crate::status;

/// §5.6's `fan_in`.
const FAN_IN: usize = 64;

/// What [`reduce_round`] made: the edit set and its id, and its declared
/// changes and their id if it is accepted.
pub type Reduced = (Digest, EditSet, Option<(Digest, Declared)>);

/// Runs reduce over round `round` of `plan` (id `plan_id`) for the
/// observers at `selected` (indices of the plan, from
/// `selected_of_round`) and the request `request`, on `threads` threads
/// with the store's scratch directory for its runs; publishes the edit set
/// (`EDTS`) and, if it is accepted, its declared changes (`DECL`). The
/// edit set depends on neither `threads` nor the scratch directory (§5.3).
///
/// A shard reduce refuses, a shard of another request included (§5.7), is
/// an `Input` error naming the observer, its main file and its shard.
pub fn reduce_round(
    store: &Store,
    plan_id: Digest,
    plan: &Plan,
    round: u64,
    selected: &[usize],
    request: (Digest, &Transform),
    threads: usize,
) -> Result<Reduced, CommandError> {
    let shards = StoreShards::new(store, plan_id, plan, round, selected);
    let scratch = store.scratch_dir().map_err(|e| store_error(store, &e))?;
    let options = Options { fan_in: FAN_IN, threads: threads.max(1), scratch };
    let reduced = slip_reduce::reduce(request.1, request.0, plan_id, round, &shards, &options)
        .map_err(|e| match e {
            ReduceError::Input { observer, message } => {
                let main = plan
                    .observers
                    .iter()
                    .find(|o| o.id == observer)
                    .map_or_else(String::new, |o| super::store::lossy(&o.main.path));
                let path = store.shard_path(&plan_id, slip_model::Phase::Map, round, &observer);
                CommandError::Input(format!(
                    "{}: the map shard of observer {observer} ({main}): {message}",
                    path.display()
                ))
            }
            e @ (ReduceError::Io(_) | ReduceError::Options(_)) => {
                CommandError::Input(e.to_string())
            }
        })?;
    let set_id = store
        .publish_object(Object::EditSet, &reduced.edits.to_bytes())
        .map_err(|e| store_error(store, &e))?;
    let declared = match reduced.declared {
        Some(d) => {
            let id = store.publish_declared(&d.to_bytes()).map_err(|e| store_error(store, &e))?;
            Some((id, d))
        }
        None => None,
    };
    Ok((set_id, reduced.edits, declared))
}

/// `slip reduce` (§10.2): status 0 for an accepted set, 1 for a refused
/// one (§10.5).
pub(crate) fn main(
    a: &ReduceArgs,
    out: &mut dyn Write,
    _err: &mut dyn Write,
) -> commands::Result<u8> {
    let dir = a.store.clone().unwrap_or_else(|| PathBuf::from(DEFAULT_STORE));
    // Exclusive: a store with a journal is refused (`StoreError::UnfinishedWrite`).
    let store = open_store(&dir, Access::Exclusive)?;
    // From here an interruption removes this process's `tmp/` entries (the
    // runs of reduce are in its scratch directory) and the lock (§7).
    signals::install(Box::new(exec_store::cleanup_on_signal(&store)));
    let PlanSource::Plan(x) = &a.plan else {
        return Err(Failure::Usage("`reduce` needs `--plan`".to_owned()));
    };
    let (plan, plan_id) = load_plan(&store, x)?;
    let selected = selected_of_round(&store, plan_id, &plan, a.round).map_err(failure)?;
    let shards = StoreShards::new(&store, plan_id, &plan, a.round, &selected);
    let (request_id, request) =
        request_of_round(&store, plan_id, a.round, &shards).map_err(failure)?;

    // The CPU tokens that can be had without waiting, at least one thread
    // (as `plan` hashes, §8.1).
    let tokens = token_pool(a.jobs);
    let mut held = Vec::new();
    while held.len() < tokens.jobs() {
        match tokens.try_acquire() {
            Some(t) => held.push(t),
            None => break,
        }
    }
    let threads = held.len().max(1);
    let reduced =
        reduce_round(&store, plan_id, &plan, a.round, &selected, (request_id, &request), threads);
    drop(held);
    let (set_id, set, declared) = reduced.map_err(failure)?;

    edit_set_summary(
        set_id,
        &set,
        declared.as_ref().map(|(id, d)| (*id, d)),
        request.indivisible,
        out,
    );
    let record = store.object_path(Object::EditSet, &set_id);
    edit_set_details(&set, &plan, None, &DiskFiles, &record, out);
    Ok(if set.state == SetState::Accepted { status::OK } else { status::FINDINGS })
}
