//! The map job of a transform (`spec/edits.md` §8.1; `spec/exec.md` §5.2,
//! §8.4): the map task's parse and walk, then, in the same parse, the
//! transform over the same input. Work package WP-074, item 1.
//!
//! What a map job with a transform adds to M1's shard is the `edits`
//! section (17) and the `edits.*` counters (`spec/edits.md` §6.3), in a
//! shard that is `ok`; everything else is the map task's
//! ([`super::map_observer`]), which a job without a transform runs
//! unchanged.

use slip_digest::Input;
use slip_digest::query::Keys;
use slip_exec::store::{Object, Store};
use slip_model::edits::Edits;
use slip_model::formats::content_id;
use slip_model::transform::TransformKind;
use slip_model::{Counter, Digest, Transform};

/// The message of a job whose request is a script, until work package
/// WP-092 adds its branch at [`run_transform`]'s dispatch.
pub const NO_SCRIPT: &str = "a script transform needs a worker built with slip_script";

/// The `edits` section and counters for a finished unit; called inside the
/// closure of the parse, after the digest walk, on the same input
/// (`spec/edits.md` §8.1: the walk's output is complete before the
/// transform is entered).
///
/// `id` is the job's request id, the content id of the `TRFM` bytes read
/// ([`read_transform`]); the section names it.
///
/// The transform is chosen by the request's kind: `rename` is
/// `slip_refactor::run_counted` with keys over `input` and no rules (the
/// pre-image's key space) and the `FileRef` of each buffer that is an
/// input; `script` is `Err` ([`NO_SCRIPT`]).
///
/// # Errors
///
/// The script's; `slip_refactor`'s (a failure of Slip: §4.6, the native
/// rename has no failed run).
pub fn run_transform(
    transform: &Transform,
    id: Digest,
    unit: &slip_kiln::Unit<'_>,
    input: &Input<'_>,
) -> Result<(Edits, Vec<Counter>), String> {
    match transform.kind {
        TransformKind::Rename => {
            let mut keys = Keys::new(input);
            let inputs = unit.inputs();
            // `Sites::file_ref`'s answer, which `slip_refactor` documents
            // the worker passes: the input whose path is the buffer's real
            // path. A buffer with a real path is one the unit read (Kiln
            // lists no virtual buffer among the inputs).
            let files = |file| {
                let path = unit.real_path(file)?;
                let at = inputs.binary_search_by(|f| f.path.as_slice().cmp(path)).ok()?;
                Some(inputs[at].clone())
            };
            slip_refactor::run_counted(transform, id, unit, &mut keys, &files)
        }
        TransformKind::Script => Err(NO_SCRIPT.to_owned()),
    }
}

/// The request `id` from the store (`Object::Transform`): decoded with
/// every rule of `TRFM`, checked to have the content id `id`, and its
/// ownership's entries checked to exist and be spelled on disk
/// ([`crate::commands::entries_on_disk`], `spec/edits.md` §2.1), before
/// anything is parsed.
///
/// # Errors
///
/// A failure of the run, never a shard (`docs/wp/WP-074.md` "Interface"):
/// the store cannot be read, the request is not there, it is not a valid
/// `TRFM`, its content id is not `id`, or an ownership entry does not
/// exist or is not spelled on disk.
pub fn read_transform(store: &Store, id: &Digest) -> Result<Transform, String> {
    let path = store.object_path(Object::Transform, id);
    let bytes = store
        .read_object(Object::Transform, id)
        .map_err(|e| format!("cannot read the request {id}: {e}"))?
        .ok_or_else(|| format!("the request {id} is not in the store ({})", path.display()))?;
    let transform = Transform::from_bytes(&bytes)
        .map_err(|e| format!("{} is not a valid TRFM file: {e}", path.display()))?;
    match content_id(&bytes) {
        Ok(actual) if actual == *id => {
            crate::commands::entries_on_disk(&transform.ownership)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(transform)
        }
        Ok(actual) => Err(format!("{} has the content id {actual}, not {id}", path.display())),
        Err(e) => Err(format!("{} is not a valid TRFM file: {e}", path.display())),
    }
}
