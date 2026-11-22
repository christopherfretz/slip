//! Response-file expansion (`spec/cdb.md` §4). Work package WP-003.

use std::path::Path;

use slip_model::Reason;

use crate::normalise::Exclusion;
use crate::split::split_response;

/// §4: replacement is recursive to this depth. A response file named on the
/// command line is at depth 1; one it names is at depth 2.
pub const MAX_DEPTH: usize = 16;

/// Replaces every argument `@path` other than `argv[0]` by the words of the
/// file `path`, recursively (`spec/cdb.md` §4).
///
/// Relative paths, at every depth, are relative to `directory` (the entry's
/// working directory). An argument that is `@` alone names no file and is
/// kept as it is.
///
/// Errors, all `malformed_entry`: a file cannot be read, is not valid UTF-8,
/// or is malformed by the GNU rules; nesting exceeds [`MAX_DEPTH`].
pub fn expand(directory: &Path, arguments: &[String]) -> Result<Vec<String>, Exclusion> {
    let mut out = Vec::with_capacity(arguments.len());
    let mut arguments = arguments.iter();
    if let Some(argv0) = arguments.next() {
        out.push(argv0.clone());
    }
    for argument in arguments {
        expand_one(directory, argument, 1, &mut out)?;
    }
    Ok(out)
}

fn expand_one(
    directory: &Path,
    argument: &str,
    depth: usize,
    out: &mut Vec<String>,
) -> Result<(), Exclusion> {
    let Some(name) = argument.strip_prefix('@').filter(|name| !name.is_empty()) else {
        out.push(argument.to_owned());
        return Ok(());
    };
    if depth > MAX_DEPTH {
        return Err(malformed(format!(
            "response files nested deeper than {MAX_DEPTH} at `{argument}`"
        )));
    }
    let path = directory.join(name);
    let bytes = std::fs::read(&path)
        .map_err(|e| malformed(format!("cannot read response file `{}`: {e}", path.display())))?;
    let text = String::from_utf8(bytes).map_err(|e| {
        malformed(format!(
            "response file `{}` is not valid UTF-8 (byte {})",
            path.display(),
            e.utf8_error().valid_up_to()
        ))
    })?;
    let words = split_response(&text)
        .map_err(|e| malformed(format!("response file `{}` is malformed: {e}", path.display())))?;
    for word in &words {
        expand_one(directory, word, depth + 1, out)?;
    }
    Ok(())
}

fn malformed(detail: String) -> Exclusion {
    Exclusion { reason: Reason::MalformedEntry, detail }
}
