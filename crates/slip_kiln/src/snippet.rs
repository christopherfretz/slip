//! A front end over files held in memory, for tests (work package WP-021).
//!
//! [`parse`] runs Kiln's preprocessor, parser and semantic analysis over a
//! [`Snippet`] and hands the finished translation unit to a closure. It is
//! not the production parse ([`crate::parse_observer`]): there is no
//! command line, no predefined macro, no system header and no file system.
//! It exists so that a test of something that walks Kiln's AST is a few
//! lines and can run on many threads at once.
//!
//! # The pipeline
//!
//! Built on Kiln's public pipeline, as `kiln -std=<std> -fsyntax-only`
//! drives it (`SyntaxJob::run` and `SyntaxJob::parse` in `kiln_driver`):
//! the files are added with `SourceManager::add_virtual` as they are first
//! included, a `Preprocessor` gets this module's own `IncludeResolver` over
//! the snippet's files, and `kiln_parse::parse_preprocessed` parses and
//! analyses the unit, its end-of-unit work included (pending
//! instantiations, `act_on_end_of_translation_unit`).
//!
//! `SyntaxJob::after_parse` runs after that in `kiln -fsyntax-only`. With no
//! other flag none of its steps changes the AST, so none is repeated here:
//! the module-file and PCH output (`write_module_file`, only with a module
//! output), the preprocessor drain to the end of the main file (only under
//! `-verify` or `-M*`; it adds files and diagnostics, not declarations), the
//! image notes and census, the constant-evaluation report and profile, the
//! instantiation report, the memory report, the CFG dump, the record- and
//! vtable-layout dumps (which can fill the AST's layout caches, but only
//! under `-fdump-record-layouts` / `-fdump-vtable-layouts`) and the AST
//! dump. Nothing of it is left out for want of a public function.
//!
//! # Language options
//!
//! Those of `kiln -std=<std> [--target=<target>] <args> -fsyntax-only`
//! ([`parse`]: no target and no argument; [`parse_with`]: those of an
//! [`Extra`]), computed by Kiln's public functions from Kiln's own parse
//! of that command line (`kiln_driver::args::Args::parse_as`) for the
//! target the line names (`x86_64-unknown-linux-gnu` without one), as
//! the private `SyntaxJob::target`, `SyntaxJob::parse_options`,
//! `Job::pp_options`, `Job::configure_preprocessor` and
//! `Job::clang_features` of `kiln_driver` compute them: Sema's target
//! (`kiln_sema::target::Target::from_triple`, then `-f[un]signed-char`,
//! `-fnew-alignment=` and `cx16`'s 16-byte atomics from
//! `kiln_driver::features::resolve`), the parser's options field by field
//! (the target features go to `target_features` on `AArch64` and
//! `x86_features` on x86-64; `complex_promoted` from
//! `Args::complex_range`'s spelling, the type being `kiln_codegen`'s),
//! the preprocessor's, `#if`'s integer model for the triple, and the
//! feature queries from the arguments without the `-fsanitize=` ones,
//! the resolved sanitizers, `-ffp-eval-method=`'s value and the target's
//! builtins (NEON on `AArch64`, x86 on x86-64) enabled by the resolved
//! features.
//!
//! Where they differ:
//!
//! - **No predefined macros**: the predefines buffer is empty (`kiln` reads
//!   it from its resource directory), so `__cplusplus`, `__x86_64__`,
//!   `__aarch64__`, `__GNUC__` and the rest are not defined, whatever the
//!   arguments and the target. The builtin macros the preprocessor
//!   defines itself (`__FILE__`, `__LINE__`, `__COUNTER__`,
//!   `__has_include`, `__FLT_EVAL_METHOD__`, …) are.
//! - **No header search**: includes resolve only among the snippet's files
//!   (see [`Snippet::files`]); no file is ever system, and `#embed` finds
//!   nothing.
//! - **Modules**: from C++20 the preprocessor has
//!   `kiln_pp::ModuleOptions::default()`, as `kiln_parse::parse_tu` does:
//!   no module file, no prebuilt path, no `std` module directory or cache
//!   (those are found on the file system), so an `import` is "module not
//!   found".
//! - **`__has_builtin` and `__has_constexpr_builtin`** answer from
//!   `kiln_pp::ClangFeatures` for the target, without the driver's
//!   staging of the builtins Kiln does not handle yet (`StagedFeatures`,
//!   `kiln_driver/src/pp_cmd.rs:1247`, is private); the other feature
//!   queries are the driver's.
//! - **The note after a missing `<meta>`** at C++26 (`PpOptions::meta_hint`)
//!   is not given: it depends on the GCC installation on the host.
//! - **`__DATE__`, `__TIME__` and `__TIMESTAMP__`** are those of the Unix
//!   epoch, as under `kiln -fkiln-reproducible-time`: a snippet parses to
//!   the same translation unit whenever and wherever it is parsed.
//! - The options that only module, PCH or header-unit output sets
//!   (`record_carried`, `record_image`, `prefix`, `header_unit`) are
//!   `false`: the arguments that would set them are refused.
//! - **The driver's own diagnostics** (warnings about the command line:
//!   `-Woption-ignored`, `-Wunused-command-line-argument`, the
//!   floating-point overrides) are not in [`Parsed::diagnostics`], which
//!   holds the job's. A driver error, and a driver warning the `-W`
//!   options make an error (`-Werror -mno-outline-atomics` on x86-64), is
//!   an `Err` of [`parse_with`], as `kiln` stops before the job for it.
//! - **The sanitizers' default ignorelists** (`clang_rt`'s, in Kiln's
//!   resource directory) are not read, so a malformed one is no error:
//!   `kiln` reads them from the file system (`Sanitize::ignorelist_errors`).
//! - **`KILN_BACKEND`** is not read (`hod::selected` reads the
//!   environment); `-fkiln-backend=` is refused.
//!
//! # Extra arguments and targets
//!
//! [`parse_with`]'s command line is `-std=<std>`, then
//! `--target=<target>` when [`Extra::target`] is given (as written: the
//! driver reads the spelling, an Android triple reserves x18), then
//! [`Extra::args`] in order, then `-fsyntax-only`. Before anything is
//! parsed:
//!
//! - **The target** must be one `kiln_driver::toolchain::Target::parse`
//!   knows: `aarch64`, `arm64`, `x86_64` or `amd64`, alone or followed by
//!   at most three `-`-separated parts of which the first or the second
//!   begins `linux` (`aarch64-unknown-linux-gnu`, `aarch64-linux-gnu`,
//!   `arm64-linux`, `x86_64-pc-linux-gnu`, `x86_64`). The first two are
//!   `aarch64-unknown-linux-gnu`, the others `x86_64-unknown-linux-gnu`;
//!   anything else is `Err`.
//! - **Each argument alone** (the head of the line, the argument,
//!   `-fsyntax-only`) is `Err`, naming it, when
//!   - its spelling is refused: an input file (`-` or no leading `-`),
//!     `-x…`, a second `-std=`, `-target` / `--target=` (the target is
//!     [`Extra::target`]), a `native` `-march=` / `-mcpu=` / `-mtune=`
//!     (it reads the host's processor), `-fkiln-stack-size=` (the thread
//!     rules below), `-fsyntax-only` (already last on the line),
//!     `-Xpreprocessor` (it takes the next argument, and last on a line
//!     the driver keeps it silently);
//!   - it takes the next argument as its value (`-Xclang`, `-o`,
//!     `-target-feature`, `-mllvm`): last on a line it is the driver's
//!     missing value, and before `-fsyntax-only` it would take that;
//!   - Kiln's driver reports an error for it: one of `Args::errors`
//!     (an unknown argument, a missing or bad value: an argument whose
//!     value is the next one, `-target-feature +sve`, must be written
//!     joined where the driver allows it, or is refused), or one of the
//!     errors `kiln_driver::run_as` reports before the job runs
//!     (`features::resolve`, the sanitizer checks, the code-generation
//!     invocation checks), or the ones it reports before the job
//!     otherwise: an error among the diagnostic options' own
//!     (`-fdiagnostics-format=sarif`), a driver warning the `-W` options
//!     make an error, the per-input errors for a C++ input
//!     (`-fvisibility=bogus`, `-fgnu89-inline`).
//!     `kiln_driver::hod::selected` is not asked: it reads the
//!     environment;
//!   - it changes a field of `Args` that says what is read or written
//!     rather than the language (`io_change`, which names every field of
//!     `Args`): an input, `-o`, `-D`/`-U` (a test writes `#define`),
//!     `-I` and every search path and toolchain directory, `-include`,
//!     the PCH and module options (`-fmodule-*`, `-fprebuilt-*`,
//!     `--precompile`, `-fmodules-cache-path=`), the action (`-c`, `-S`,
//!     `-E`, `-emit-llvm`), `-M*`, `-verify`, `--driver-mode=`, `-ansi`,
//!     the dumps and reports (`-fdump-*`, `-fkiln-dump-cfg`,
//!     `-fkiln-memory-report`, `-fkiln-pp-record`, `-ftime-trace`, `-fconstexpr-report=`,
//!     `-fconstexpr-profile*`, `-fkiln-instantiation-report`), the crash
//!     reproducer's options, `-fkiln-backend=`, the module-file kinds,
//!     the coverage options, the sanitizer ignore- and coverage lists,
//!     `-###`, `-v`, `--help`, the probes, the link and assembler options,
//!     and `-fdiagnostics-format=json` (the diagnostics are text);
//! - **The whole line** is `Err` when the driver reports an error for the
//!   arguments together (`-fno-rtti -fsanitize=vptr`, `-Werror
//!   -ffp-model=fast -ffp-contract=on`), or when they together change
//!   what is read or written.
//!
//! A snippet stays the only input and its unit's AST the only output.
//! Every other argument the driver knows is accepted and reaches the
//! front end only through the options above; one that changes only code
//! generation (`-O2`, `-g`, `-fPIC`) is accepted and changes nothing.

//! # Threads
//!
//! The front end runs on a thread of its own with the stack `kiln` gives
//! its compile thread (`-fkiln-stack-size`'s default) and Kiln's stack
//! guard installed on it, as [`crate::parse_observer`] runs it: nesting too
//! deep for the stack is Kiln's fatal diagnostic, not a stack overflow. The
//! finished state moves back and the closure runs on the calling thread
//! (it is not required to be `Send`); the call returns when both are done.
//! Nothing is shared between calls: no current directory, no environment,
//! no file, and every thread-local of Kiln's starts fresh on the new
//! thread, so calls on many threads at once do not affect each other.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use kiln_diag::{DiagEngine, DiagId, DiagOptions, SharedBuffer, TextSink};
use kiln_driver::args::Args;
use kiln_driver::toolchain::{LangStd, StdLib, Target as DriverTarget};
use kiln_oven::Oven;
use kiln_parse::{ParseOptions, parse_preprocessed};
use kiln_pp::{
    ClangFeatures, IfEval, IncludeKind, IncludeRequest, IncludeResolver, ModuleOptions, PpOptions,
    Preprocessor, ResolvedInclude, TargetBuiltins,
};
use kiln_sema::target::Target;
use kiln_source::symbol::Interner;
use kiln_source::{FileId, SourceManager, Span};

use crate::kiln;

/// The target every snippet is parsed for.
const TRIPLE: &str = "x86_64-unknown-linux-gnu";

/// The standards a snippet may name.
const STDS: &[&str] = &["c++17", "c++20", "c++23", "c++26"];

/// A translation unit in memory.
#[derive(Clone, Debug)]
pub struct Snippet<'a> {
    /// `(path, text)`. Paths are `/`-separated and relative to an
    /// imaginary root; no two equal. `#include "x"` looks beside the
    /// including file, then in each directory of `include`, in order;
    /// `#include <x>` only in `include`.
    ///
    /// A path is normalised: not empty, no leading or trailing `/`, no
    /// empty, `.` or `..` component. A header name is joined to the
    /// directory it is looked up in and normalised (`.` dropped, `..`
    /// removing the component before it); a name that is absolute or
    /// climbs above the root is not found. `#include_next` continues in
    /// `include` after the directory the including file was found in.
    pub files: &'a [(&'a str, &'a str)],
    /// The path of the main file; one of `files`.
    pub main: &'a str,
    /// Directories (paths as above; `""` is the root) searched in order.
    pub include: &'a [&'a str],
    /// `c++17`, `c++20`, `c++23`, `c++26` (what Kiln's `-std=` accepts).
    pub std: &'a str,
}

/// The state of the translation unit when its AST is final. References
/// only, valid inside the call.
#[non_exhaustive]
pub struct Parsed<'r> {
    pub sm: &'r SourceManager,
    pub ast: &'r kiln::ast::Ast,
    pub oven: &'r Oven,
    pub main_file: FileId,
    pub errors: u32,
    pub warnings: u32,
    /// Every diagnostic as Kiln's text sink renders it, in order.
    pub diagnostics: String,
    paths: &'r HashMap<FileId, &'r str>,
    unit: crate::Unit<'r>,
}

impl std::fmt::Debug for Parsed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parsed")
            .field("main_file", &self.main_file)
            .field("errors", &self.errors)
            .field("warnings", &self.warnings)
            .field("diagnostics", &self.diagnostics)
            .finish_non_exhaustive()
    }
}

impl<'r> Parsed<'r> {
    /// The snippet path a buffer was added under; `None` for a buffer the
    /// front end made itself.
    pub fn path(&self, file: FileId) -> Option<&str> {
        self.paths.get(&file).copied()
    }

    /// The unit as [`crate::parse_observer_with`]'s closure would see it
    /// (`Unit::token_end`, `crate::ranges`): no preprocessing record and
    /// no inputs, so `Unit::real_path` is `None` for every buffer.
    pub fn unit(&self) -> &crate::Unit<'r> {
        &self.unit
    }
}

/// What a test adds to `kiln -std=<std> -fsyntax-only` (see "Extra
/// arguments and targets" in the module documentation).
#[derive(Clone, Debug, Default)]
pub struct Extra<'a> {
    /// Driver arguments, each as one would write it (`-fms-extensions`,
    /// `-march=armv9-a+sve2`). Appended after `-std=` and before
    /// `-fsyntax-only`.
    pub args: &'a [&'a str],
    /// A target triple Kiln's driver knows; `None` is
    /// `x86_64-unknown-linux-gnu`.
    pub target: Option<&'a str>,
}

/// Parses `snippet` for x86-64 Linux and calls `f` when the AST is final
/// (after everything `kiln -fsyntax-only` does to it before it would drop
/// it), with errors or without.
///
/// Err: the snippet itself is wrong (a path twice, `main` not among the
/// files, an unknown `std`), or the front end panicked (the payload's
/// text). The process's panic hook is not replaced.
///
/// A panic in `f` is an `Err` too, `the callback panicked: <payload>`; a
/// path or include directory that is not normalised (see
/// [`Snippet::files`]) is a wrong snippet.
///
/// It is [`parse_with`] with [`Extra::default()`].
pub fn parse<R>(snippet: &Snippet<'_>, f: impl FnOnce(&Parsed<'_>) -> R) -> Result<R, String> {
    parse_on(snippet, &Extra::default(), None, false, f)
}

/// [`parse`] as `kiln -std=<std> [--target=<target>] <args>
/// -fsyntax-only` would parse it (module documentation, "Extra arguments
/// and targets").
///
/// Err, besides [`parse`]'s: an argument Kiln's driver does not know or
/// reports an error for, one that changes what is read or written rather
/// than the language, one that would make the result depend on the host,
/// and a target the driver does not know; each named, before anything is
/// parsed.
pub fn parse_with<R>(
    snippet: &Snippet<'_>,
    extra: &Extra<'_>,
    f: impl FnOnce(&Parsed<'_>) -> R,
) -> Result<R, String> {
    parse_on(snippet, extra, None, false, f)
}

/// [`parse_with`] with Kiln's written-reference table (Kiln `spec/ast.md`
/// §18) and declaration-extent table (§19) recorded, as
/// `-fkiln-dump-written-refs -fkiln-decl-extents` record them without
/// printing anything: the tests of what reads them (work package WP-084,
/// test 6). Nothing else changes.
pub fn parse_with_tables<R>(
    snippet: &Snippet<'_>,
    extra: &Extra<'_>,
    f: impl FnOnce(&Parsed<'_>) -> R,
) -> Result<R, String> {
    parse_on(snippet, extra, None, true, f)
}

/// [`parse_with`] with the front end's thread given `stack` bytes instead
/// of `kiln`'s default (`-fkiln-stack-size=`; the tests of the stack
/// guard), and the two tables of [`parse_with_tables`] when `tables`.
fn parse_on<R>(
    snippet: &Snippet<'_>,
    extra: &Extra<'_>,
    stack: Option<usize>,
    tables: bool,
    f: impl FnOnce(&Parsed<'_>) -> R,
) -> Result<R, String> {
    let checked = check(snippet)?;
    let mut lang = Language::new(snippet.std, extra)?;
    if let Some(bytes) = stack {
        lang.line.push(format!("-fkiln-stack-size={bytes}"));
    }
    let done = on_front_end_thread(&lang, || front_end(snippet, &checked, &lang, tables))?;
    let paths: HashMap<FileId, &str> =
        done.loaded.iter().map(|&(file, index)| (file, snippet.files[index].0)).collect();
    let parsed = Parsed {
        sm: &done.sm,
        ast: &done.ast,
        oven: &done.oven,
        main_file: done.main_file,
        errors: done.errors,
        warnings: done.warnings,
        diagnostics: done.diagnostics,
        paths: &paths,
        unit: crate::Unit::of_snippet(
            &done.ast,
            &done.sm,
            &done.oven,
            &done.interner,
            done.lex,
            done.main_file,
            done.errors,
        ),
    };
    catch_unwind(AssertUnwindSafe(|| f(&parsed)))
        .map_err(|payload| format!("the callback panicked: {}", text_of(&*payload)))
}

/// Runs `work` on a thread with the stack `kiln` gives its compile thread
/// for `lang`'s line, with Kiln's stack guard installed (module
/// documentation, "Threads").
fn on_front_end_thread<T: Send>(
    lang: &Language,
    work: impl FnOnce() -> Result<T, String> + Send,
) -> Result<T, String> {
    let size = kiln_driver::templ_opts::stack_size_of(&lang.line);
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("kiln".into())
            .stack_size(size)
            .spawn_scoped(scope, || {
                // As the `kiln` executable's compile thread does.
                kiln_sema::stack_guard::StackGuard::install(size);
                work()
            })
            .map_err(|e| {
                format!("cannot start the front end's thread with a {size}-byte stack: {e}")
            })?;
        handle.join().map_err(|payload| format!("the front end panicked: {}", text_of(&*payload)))
    })?
}

/// The tokens of a snippet after translation phase 4, as [`preprocess`]
/// hands them over. References only, valid inside the call.
#[non_exhaustive]
pub struct Preprocessed<'r> {
    pub sm: &'r SourceManager,
    pub main_file: FileId,
    /// Every token the preprocessor delivered, in order, without the end
    /// of file.
    pub tokens: &'r [kiln::lex::Token],
    /// The errors the preprocessor reported.
    pub errors: u32,
    paths: &'r HashMap<FileId, &'r str>,
}

impl std::fmt::Debug for Preprocessed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Preprocessed")
            .field("main_file", &self.main_file)
            .field("tokens", &self.tokens.len())
            .finish_non_exhaustive()
    }
}

impl Preprocessed<'_> {
    /// As [`Parsed::path`].
    pub fn path(&self, file: FileId) -> Option<&str> {
        self.paths.get(&file).copied()
    }
}

/// The preprocessor of [`parse_with`] alone over `snippet`: its tokens,
/// as the parser would have received them, handed to `f` (`kiln -E`
/// without the printing). The source manager is built as the parse
/// builds it, so for a snippet the parse reads to its end the locations
/// are the parse's (work package WP-084, the property of its test 4).
///
/// Err as [`parse_with`]; a panic in `f` is an `Err`.
pub fn preprocess<R>(
    snippet: &Snippet<'_>,
    extra: &Extra<'_>,
    f: impl FnOnce(&Preprocessed<'_>) -> R,
) -> Result<R, String> {
    let checked = check(snippet)?;
    let lang = Language::new(snippet.std, extra)?;
    let done = on_front_end_thread(&lang, || preprocess_only(snippet, &checked, &lang))?;
    let paths: HashMap<FileId, &str> =
        done.loaded.iter().map(|&(file, index)| (file, snippet.files[index].0)).collect();
    let pre = Preprocessed {
        sm: &done.sm,
        main_file: done.main_file,
        tokens: &done.tokens,
        errors: done.errors,
        paths: &paths,
    };
    catch_unwind(AssertUnwindSafe(|| f(&pre)))
        .map_err(|payload| format!("the callback panicked: {}", text_of(&*payload)))
}

/// What [`preprocess_only`] hands back across the join.
struct PpDone {
    sm: SourceManager,
    main_file: FileId,
    tokens: Vec<kiln::lex::Token>,
    errors: u32,
    loaded: Vec<(FileId, usize)>,
}

/// [`front_end`]'s preprocessor, run to the end of the main file.
fn preprocess_only(
    snippet: &Snippet<'_>,
    checked: &Checked<'_>,
    lang: &Language,
) -> Result<PpDone, String> {
    let sm = SourceManager::new();
    sm.set_macro_prefix_map(&lang.args.macro_prefix_map);
    sm.set_source_path_style(lang.args.source_path_style);
    let interner = Arc::new(Interner::new());
    let diag_opts = lang.diag_options();
    let features = lang.features()?;
    let (main_path, main_text) = snippet.files[checked.main];
    let main_file = sm
        .add_virtual(main_path, main_text.to_owned())
        .map_err(|_| "the source manager's location space is exhausted".to_owned())?;
    let loaded = std::rc::Rc::new(std::cell::RefCell::new(vec![(main_file, checked.main)]));
    let (tokens, errors) = {
        let engine =
            DiagEngine::new(&sm, &diag_opts, TextSink::new(SharedBuffer::new(), &diag_opts));
        let mut pp = Preprocessor::new(&sm, &engine, &interner, lang.pp_options());
        configure(&mut pp, snippet, checked, lang, &loaded, features);
        pp.enter_main_file(main_file);
        let mut tokens = Vec::new();
        loop {
            let t = pp.next();
            if t.is_eof() {
                break;
            }
            tokens.push(t);
        }
        drop(pp);
        (tokens, engine.error_count())
    };
    let loaded = loaded.borrow().clone();
    Ok(PpDone { sm, main_file, tokens, errors, loaded })
}

/// What [`front_end`] and [`preprocess_only`] set on their preprocessor
/// before entering the main file.
fn configure<'d>(
    pp: &mut Preprocessor<'_, 'd>,
    snippet: &'d Snippet<'d>,
    checked: &'d Checked<'d>,
    lang: &Language,
    loaded: &std::rc::Rc<std::cell::RefCell<Vec<(FileId, usize)>>>,
    features: ClangFeatures,
) {
    pp.set_include_resolver(Box::new(Resolver {
        snippet,
        index: &checked.index,
        loaded: std::rc::Rc::clone(loaded),
    }));
    pp.set_if_evaluator(Box::new(if_evaluator(lang)));
    pp.set_feature_queries(Box::new(features));
}

/// The text of a panic's payload.
fn text_of(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic".into())
}

/// A snippet found well formed: its paths, by path.
struct Checked<'a> {
    index: HashMap<&'a str, usize>,
    main: usize,
}

fn check<'a>(snippet: &Snippet<'a>) -> Result<Checked<'a>, String> {
    let mut index = HashMap::with_capacity(snippet.files.len());
    for (i, &(path, _)) in snippet.files.iter().enumerate() {
        if !normalised(path) {
            return Err(format!("snippet path `{path}` is not a normalised relative path"));
        }
        if index.insert(path, i).is_some() {
            return Err(format!("snippet path `{path}` is given twice"));
        }
    }
    let Some(&main) = index.get(snippet.main) else {
        return Err(format!("the main file `{}` is not among the snippet's files", snippet.main));
    };
    if let Some(dir) = snippet.include.iter().find(|d| !d.is_empty() && !normalised(d)) {
        return Err(format!("include directory `{dir}` is not a normalised relative path"));
    }
    Ok(Checked { index, main })
}

/// Not empty, and every `/`-separated component neither empty nor `.` or
/// `..` (so no leading or trailing `/`).
fn normalised(path: &str) -> bool {
    !path.is_empty() && path.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// `name` looked up in `dir` (`""` the root), normalised; `None` for an
/// absolute name or one that climbs above the root.
fn join(dir: &str, name: &str) -> Option<String> {
    if name.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|c| !c.is_empty()).collect();
    for c in name.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    Some(parts.join("/"))
}

/// The directory of a snippet path (`""` for one at the root).
fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// What the front end's thread hands back: owned, so it can cross the join.
struct Done {
    sm: SourceManager,
    ast: kiln_ast::Ast,
    oven: Oven,
    main_file: FileId,
    errors: u32,
    warnings: u32,
    diagnostics: String,
    /// Every snippet file added to `sm`, with its index in the snippet.
    loaded: Vec<(FileId, usize)>,
    /// The identifiers' interner and the preprocessor's lexer options,
    /// for [`Parsed::unit`].
    interner: Arc<Interner>,
    lex: kiln_lex::LexOptions,
}

/// The language options of `kiln -std=<std> [--target=<t>] <args>
/// -fsyntax-only`, as far as Kiln's public functions give them.
struct Language {
    /// The command line they come from (without the program name).
    line: Vec<String>,
    args: Args,
    std: LangStd,
    target: DriverTarget,
}

impl Language {
    fn new(std: &str, extra: &Extra<'_>) -> Result<Language, String> {
        let lang = STDS.contains(&std).then(|| LangStd::parse(std)).flatten();
        let Some(lang) = lang else {
            return Err(format!("unknown std `{std}`; one of {}", STDS.join(", ")));
        };
        let target = match extra.target {
            None => {
                let Some(t) = DriverTarget::parse(TRIPLE) else {
                    unreachable!("Kiln's driver knows the triple {TRIPLE}");
                };
                t
            }
            Some(t) => DriverTarget::parse(t).ok_or_else(|| {
                format!(
                    "unknown target `{t}`: Kiln's driver knows an `aarch64`, `arm64`, `x86_64` or \
                     `amd64` Linux triple (`DriverTarget::parse`)"
                )
            })?,
        };
        let mut head = vec![format!("-std={std}")];
        if let Some(t) = extra.target {
            head.push(format!("--target={t}"));
        }
        let line_of = |args: &[&str]| -> Vec<String> {
            let mut line = head.clone();
            line.extend(args.iter().map(|a| (*a).to_owned()));
            line.push("-fsyntax-only".to_owned());
            line
        };
        let base = Args::parse_as("kiln", &line_of(&[]));
        if let Some(e) = base.errors.first() {
            unreachable!("Kiln's driver rejects `-std={std}` for {}: {e:?}", target.triple);
        }
        // Each argument alone, so that the one refused is named.
        for &arg in extra.args {
            if let Some(why) = refused_spelling(arg) {
                return Err(format!("argument `{arg}` is refused: {why}"));
            }
            // Last on a line, an option that takes the next argument as
            // its value (`-Xclang`, `-o`, `-target-feature`) is missing
            // it: it would take `-fsyntax-only` or the next argument.
            let mut last = head.clone();
            last.push(arg.to_owned());
            let last = Args::parse_as("kiln", &last);
            if last.errors.iter().any(|e| matches!(e, kiln_driver::args::ArgError::MissingValue(_)))
            {
                return Err(format!(
                    "argument `{arg}` is refused: it takes the next argument as its value \
                     (write the value joined, where the driver allows it)"
                ));
            }
            let alone = Args::parse_as("kiln", &line_of(&[arg]));
            if let Some(e) = driver_errors(&alone, &target).first() {
                return Err(format!("argument `{arg}`: Kiln's driver reports {e}"));
            }
            if let Some(e) = driver_reports(&alone, &target) {
                return Err(format!("argument `{arg}`: Kiln's driver stops before the job: {e}"));
            }
            if let Some(field) = io_change(&base, &alone) {
                return Err(format!(
                    "argument `{arg}` is refused: it changes what is read or written \
                     (`Args::{field}`), and a snippet's files are its only input"
                ));
            }
        }
        let line = line_of(extra.args);
        let args = Args::parse_as("kiln", &line);
        let together = extra.args.join(" ");
        if let Some(e) = driver_errors(&args, &target).first() {
            return Err(format!("arguments `{together}` together: Kiln's driver reports {e}"));
        }
        if let Some(e) = driver_reports(&args, &target) {
            return Err(format!(
                "arguments `{together}` together: Kiln's driver stops before the job: {e}"
            ));
        }
        if let Some(field) = io_change(&base, &args) {
            return Err(format!(
                "arguments `{together}` together are refused: they change what is read or \
                 written (`Args::{field}`), and a snippet's files are its only input"
            ));
        }
        Ok(Language { line, args, std: lang, target })
    }

    fn x86(&self) -> bool {
        !self.target.is_aarch64()
    }

    /// The diagnostic options (`run_as`).
    fn diag_options(&self) -> DiagOptions {
        diag_options_of(&self.args)
    }

    /// Sema's target (`SyntaxJob::target`).
    fn sema_target(&self) -> Result<Target, String> {
        let a = &self.args;
        let mut target = Target::from_triple(self.target.triple)
            .map_err(|_| format!("Kiln has no target {}", self.target.triple))?;
        target.types.char_signed = a.predef.char_signed.unwrap_or(target.types.char_signed);
        target.default_new_align = a.predef.new_alignment.unwrap_or(target.default_new_align);
        if kiln_driver::features::resolve(a, &self.target).0.x86_cx16() {
            target.types.max_atomic_inline_bytes = 16;
        }
        Ok(target)
    }

    /// The parser's options (`SyntaxJob::parse_options`).
    fn parse_options(&self) -> ParseOptions {
        let a = &self.args;
        let std = &self.std;
        let x86 = self.x86();
        let feats = kiln_driver::features::resolve(a, &self.target).0;
        ParseOptions {
            exceptions: a.predef.exceptions.unwrap_or(std.cplusplus),
            rtti: a.predef.rtti.unwrap_or(std.cplusplus),
            access_control: a.predef.access_control.unwrap_or(true),
            inst_limits: a.templ.limits(),
            inst_report: a.templ.report,
            reflection: a.predef.reflection_for(std),
            contracts: Some(a.predef.contracts_for(std)),
            coro_aligned_allocation: a.predef.coro_aligned_allocation,
            lax_vector_conversions: a.lax_vector_conversions,
            clang_compat: a.predef.clang_compat.major(),
            gnu_lib_builtins: a.stdlib.unwrap_or_default() == StdLib::LibStdCxx,
            no_builtin: a.no_builtin(),
            no_builtin_fns: a.no_builtin_fns(),
            freestanding: a.predef.freestanding.unwrap_or(false),
            sized_deallocation: a.predef.sized_deallocation_for(std),
            aligned_allocation: a.predef.aligned_allocation_for(std),
            short_enums: a.predef.short_enums,
            char8: a.predef.char8_t,
            fp: a.fp_model().opts,
            fp_eval_on_command_line: a.fp_model().eval_on_command_line,
            // `a.complex_range() == ComplexRange::Promoted`: the type is
            // `kiln_codegen`'s, which this crate does not link; its
            // spelling says the same.
            complex_promoted: a.complex_range().name() == "promoted",
            sve: feats.sve,
            sve_vector_bits: feats.sve_vector_bits,
            aarch64_cpu_valid: kiln_driver::features::aarch64_attr_cpu_valid,
            target_features: if x86 { Vec::new() } else { feats.extra.clone() },
            x86_features: if x86 { feats.extra.clone() } else { Vec::new() },
            target_attr_features: kiln_driver::features::target_attr_feature_list,
            cfg_dump: a.dump_cfg,
            cfg_stats: a.cfg_stats,
            cfg_max_elems: a.cfg_max_elems.unwrap_or(kiln_sema::cfg::DEFAULT_MAX_ELEMS),
            permissive: a.permissive,
            range_for_ext_temps: a.range_for_ext_temps,
            contract_semantic: a.contract_semantic.unwrap_or_default(),
            contract_checks: a.contract_checks,
            cf_protection_branch: a.codegen.cf_protection_branch(x86),
            // Set only when a module file is written.
            record_carried: false,
            record_image: false,
            image_census: a.bmi_image_census,
            gnu89_inline: a.predef.gnu89_inline_for(std),
            // Set only when a PCH is produced or its lookups recorded.
            prefix: false,
            // Set only for a header unit's `--precompile`.
            header_unit: false,
            ..std.parse_options()
        }
    }

    /// The preprocessor's options (`Job::pp_options` and
    /// `Job::configure_preprocessor` for a mode that runs sema), with no
    /// predefines.
    fn pp_options(&self) -> PpOptions {
        let a = &self.args;
        let std = &self.std;
        let mut o = PpOptions::new(std.lex);
        // A test's input is its files and nothing else: never the clock.
        let (date, time, timestamp) = kiln_driver::time::spellings(0);
        o.reproducible_time = Some((date, time));
        o.timestamp = Some(timestamp);
        o.c = std.c;
        o.gnu = std.gnu;
        // `!(gnu || C++17 or later)`: every snippet standard is C++17 or
        // later, so no trigraphs unless asked for.
        o.trigraphs = a.trigraphs.unwrap_or(false);
        o.dollar_in_identifiers = a.dollars.unwrap_or(true);
        o.asm = false;
        o.digraphs = std.digraphs();
        o.keep_comments = false;
        o.keep_macro_comments = false;
        o.reflection = a.predef.reflection_for(std);
        o.char8 = a.predef.char8_t;
        o.clang19_escapes = a.predef.clang_compat == kiln_pp::ClangCompat::Clang19;
        o.main_is_header = false;
        o.meta_hint = None;
        o.predefines = String::new();
        o.parser_pragmas = true;
        o.preprocessed_output = false;
        if std.cxx20() {
            o.modules = Some(ModuleOptions::default());
        }
        o
    }

    /// The `__has_feature`-style answers (`Job::clang_features`).
    fn features(&self) -> Result<ClangFeatures, String> {
        let a = &self.args;
        let triple = self.target.triple;
        let x86 = self.x86();
        // `__has_feature(address_sanitizer)` and the rest answer from the
        // resolved sanitizers, not from the arguments as written.
        let mut fargs: Vec<String> = a
            .argv
            .iter()
            .filter(|s| {
                ![
                    "-fsanitize=",
                    "-fno-sanitize=",
                    "-fsanitize-coverage=",
                    "-fno-sanitize-coverage=",
                ]
                .iter()
                .any(|p| s.starts_with(p))
            })
            .cloned()
            .collect();
        fargs.extend(a.sanitize(x86, triple).0.feature_args());
        fargs.push(if self.std.gnu { "-std=gnu++" } else { "-std=c++" }.to_owned());
        let mut features = ClangFeatures::from_args(triple, self.std.lex, &fargs)
            .ok_or_else(|| format!("Kiln has no feature tables for {triple}"))?
            .with_compat(a.predef.clang_compat);
        let fp = a.fp_model();
        if fp.eval_on_command_line {
            // `-ffp-eval-method=`'s value, as `__FLT_EVAL_METHOD__`.
            let v = match fp.opts.eval {
                kiln_ast::fp::FpEval::Source => 0,
                kiln_ast::fp::FpEval::Double => 1,
                kiln_ast::fp::FpEval::Extended => 2,
            };
            features = features.with_flt_eval_method(v);
        }
        let resolved = kiln_driver::features::resolve(a, &self.target).0;
        let builtins = if x86 {
            TargetBuiltins {
                prefix: kiln_ast::decl::builtin::x86::PREFIX,
                required: kiln_ast::decl::builtin::x86::required_features,
                enabled: resolved.x86_enabled(),
            }
        } else {
            TargetBuiltins {
                prefix: kiln_ast::decl::builtin::neon::PREFIX,
                required: kiln_ast::decl::builtin::neon::required_features,
                enabled: resolved.enabled(),
            }
        };
        Ok(features.with_target_builtins(builtins))
    }
}

/// Why an argument is refused by its spelling alone: the ones whose effect
/// `Args` does not show (`-x` before no input, a `-std=` equal to the
/// snippet's) or that make the result depend on the host or the thread.
fn refused_spelling(arg: &str) -> Option<&'static str> {
    let value = |opt: &str| arg.strip_prefix(opt);
    if arg == "-" || !arg.starts_with('-') {
        return Some("an input file: the snippet's files are its only input");
    }
    if arg.starts_with("-x") {
        return Some("`-x` types an input; a snippet is C++ in its `Snippet::std`");
    }
    if ["-std", "--std"].iter().any(|o| arg == *o || value(o).is_some_and(|v| v.starts_with('='))) {
        return Some("a second `-std=`: the standard is `Snippet::std`");
    }
    if arg == "-target" || arg == "--target" || arg.starts_with("--target=") {
        return Some("the target is `Extra::target`");
    }
    if ["-march=", "-mcpu=", "-mtune="]
        .iter()
        .any(|o| value(o).is_some_and(|v| v.starts_with("native")))
    {
        return Some("`native` reads the host's processor: the result would depend on the host");
    }
    if arg == "-fsyntax-only" {
        return Some("`-fsyntax-only` is already the action, last on the line");
    }
    if arg.starts_with("-fkiln-stack-size=") {
        return Some("the front end's thread has `kiln`'s default stack");
    }
    if arg == "-Xpreprocessor" {
        // `splice_preprocessor_args` replaces it and the next argument by
        // that argument; last on a line it stays, and nothing reports it.
        return Some(
            "`-Xpreprocessor` takes the next argument as its value: write that argument alone",
        );
    }
    None
}

/// Every error Kiln's driver reports for the command line `a` before it
/// runs the `-fsyntax-only` job (`run_as`: `command_line_checks`,
/// `toolchain_checks`, `input_checks` and the job's own
/// `invocation_errors`), as text. The backend check (`hod::selected`) is
/// left out: it reads the environment, and `-fkiln-backend=` is refused
/// by [`io_change`].
fn driver_errors(a: &Args, target: &DriverTarget) -> Vec<String> {
    let x86 = !target.is_aarch64();
    let clang22 = a.predef.clang_compat != kiln_pp::ClangCompat::Clang19;
    let mut out: Vec<String> = a.errors.iter().map(|e| format!("{e:?}")).collect();
    let (features, feature_errors) = kiln_driver::features::resolve(a, target);
    out.extend(feature_errors.iter().map(|e| format!("{e:?}")));
    let (sanitize, sanitize_errors) = a.sanitize(x86, target.triple);
    out.extend(sanitize_errors.iter().map(|e| format!("{e:?}")));
    out.extend(
        a.sanitize.coverage_errors(sanitize.opts.kinds, clang22).iter().map(|e| format!("{e:?}")),
    );
    out.extend(sanitize.hardware_errors(!x86, features.memtag_hw).iter().map(|e| format!("{e:?}")));
    if a.codegen.large_data_threshold.is_some() && !x86 && !clang22 {
        out.push("UnsupportedForTarget(\"-mlarge-data-threshold=\")".to_owned());
    }
    out.extend(a.codegen.xray_errors().iter().map(|e| format!("{e:?}")));
    out.extend(a.codegen.invocation_errors(x86, target.triple).iter().map(|e| format!("{e:?}")));
    out
}

/// The diagnostic options of `a`, as the driver makes them (`diag_options`).
fn diag_options_of(a: &Args) -> DiagOptions {
    let mut o = DiagOptions::from_args(&a.diag_args);
    o.clang_compat = a.predef.clang_compat.major();
    o
}

/// The errors `run_as` reports before the job that [`driver_errors`] does
/// not list, rendered as `kiln` renders them (the driver's prefix
/// `kiln: `), or `None` when there is none:
///
/// - an error among the diagnostic options' own (`DiagOptions::pending`:
///   `-fdiagnostics-format=sarif`, `-ferror-limit=x`), which the driver's
///   engine (`driver_options`: no `-W`, no `-R`) reports first;
/// - a driver warning (`driver_warnings`, [`driver_warnings`] here) that
///   the `-W` options make an error (`-Werror -mno-outline-atomics` on
///   x86-64): `kiln` then runs no job;
/// - the per-input errors of `input_std` for a C++ input in the
///   snippet's standard: an invalid `-fvisibility=` value, and
///   `-fgnu89-inline` (C only). Its other errors are about `-std=` and
///   `-ansi`, which are refused.
fn driver_reports(a: &Args, target: &DriverTarget) -> Option<String> {
    let diag_opts = diag_options_of(a);
    let render = |opts: &DiagOptions, prefix: bool, diags: Vec<kiln_diag::Diag>| {
        let sm = SourceManager::new();
        let buffer = SharedBuffer::new();
        let sink = TextSink::new(buffer.clone(), opts);
        let sink = if prefix { sink.with_prefix(a.program.clone()) } else { sink };
        let e = DiagEngine::new(&sm, opts, sink);
        for d in diags {
            d.emit(&e);
        }
        e.flush();
        (e.error_count() > 0).then(|| buffer.contents().trim_end().to_owned())
    };
    // `driver_options`: the main engine, which emits the pending ones.
    let mut drv = diag_opts.clone();
    drv.warnings.clear();
    drv.remarks.clear();
    if let Some(text) = render(&drv, true, Vec::new()) {
        return Some(text);
    }
    // `codegen_cmd::driver_diag_options`: the driver warnings' engine.
    let mut warn = diag_opts.clone();
    warn.pending.clear();
    warn.remarks.clear();
    warn.warnings.retain(|w| {
        let w = w.strip_prefix("no-").unwrap_or(w);
        let w = w.strip_prefix("error=").unwrap_or(w);
        matches!(w, "error" | "everything") || kiln_diag::Group::from_name(w).is_some()
    });
    if let Some(text) = render(&warn, true, driver_warnings(a, target)) {
        return Some(text);
    }
    // `input_std` for a C++ input, on the job's options (no prefix).
    let mut job = diag_opts;
    job.pending.clear();
    let invalid = |id, x: &str, y: &str| kiln_diag::Diag::new(id, Span::INVALID).arg(x).arg(y);
    let input = if let Some(arg) = &a.invalid_visibility {
        let v = arg.strip_prefix("-fvisibility=").unwrap_or(arg);
        Some(invalid(DiagId::drv_invalid_value, arg, v))
    } else if a.predef.gnu89_inline == Some(true) {
        Some(invalid(DiagId::drv_argument_not_allowed_with, "-fgnu89-inline", "C++"))
    } else {
        None
    };
    render(&job, false, input.into_iter().collect())
}

/// The warnings `driver_warnings` (`kiln_driver/src/lib.rs`) reports for
/// the line's one C++ source input, in its order. Its others cannot occur
/// on a line [`io_change`] accepts: an input promoted from C (the input is
/// C++), the retired `-fkiln-pch-image` (`Args::pch`) and the unused phase
/// options (`Args::phase_args`).
fn driver_warnings(a: &Args, target: &DriverTarget) -> Vec<kiln_diag::Diag> {
    use kiln_diag::Diag;
    use kiln_driver::fp_flags::FpWarning;
    use kiln_driver::x86_features as x86f;
    let arm = target.is_aarch64();
    let mut out: Vec<Diag> = a
        .fp_model()
        .warnings
        .iter()
        .map(|w| match w {
            FpWarning::ComplexRange { implied_by, flag, old, new } => {
                Diag::new(DiagId::drv_overriding_complex_range, Span::INVALID)
                    .arg(implied_by)
                    .arg(flag)
                    .arg(*old)
                    .arg(*new)
            }
            FpWarning::Option(old, new) => {
                Diag::new(DiagId::drv_overriding_option, Span::INVALID).arg(old).arg(new)
            }
        })
        .collect();
    let ignored = |flag: &str| {
        Diag::new(DiagId::drv_option_ignored, Span::INVALID).arg(target.arch).arg(flag)
    };
    if let Some(on) = a.outline_atomics.filter(|_| !arm) {
        out.push(ignored(if on { "-moutline-atomics" } else { "-mno-outline-atomics" }));
    }
    if a.codegen.outline == Some(true) && !arm {
        out.push(ignored("-moutline"));
    }
    if a.codegen.large_data_threshold_ignored(!arm) {
        out.push(
            Diag::new(DiagId::drv_large_data_threshold_invalid_code_model, Span::INVALID)
                .arg("mlarge-data-threshold="),
        );
    }
    let mut unused: Vec<String> = Vec::new();
    if !a.quiet_unused_arguments {
        unused = a.codegen.unused_xray_args();
        if !arm {
            unused.extend(a.codegen.sign_return_address_written.iter().cloned());
        }
        unused.extend(kiln_driver::time_trace::unused(a).iter().cloned());
        unused.extend(a.codegen.trapv_handler_separate.iter().cloned());
        let san = a.sanitize(!arm, target.triple).0;
        unused.extend(a.sanitize.unused_coverage_lists(&san));
    }
    let v22 = a.predef.clang_compat == kiln_pp::ClangCompat::Clang22;
    let mut no_3dnow: Vec<&String> = Vec::new();
    for f in a
        .x86_feature_flags
        .iter()
        .filter(|f| !x86f::unknown_at(f, v22) && f.as_str() != x86f::GENERAL_REGS_ONLY)
    {
        if arm {
            if !a.quiet_unused_arguments && x86f::non_x86_spelling(f, v22).is_none() {
                unused.push(f.clone());
            }
        } else if x86f::is_3dnow(f) && !f.starts_with("-mno-") {
            no_3dnow.push(f);
        }
    }
    for u in unused.iter().filter(|u| !a.no_unused.quiet(u)) {
        out.push(Diag::new(DiagId::drv_argument_unused, Span::INVALID).arg(u));
    }
    for f in no_3dnow {
        out.push(Diag::new(DiagId::drv_clang_unsupported, Span::INVALID).arg(f));
    }
    out
}

/// The first field of `a` that differs from `base` and says what is read
/// or written rather than the language: an input, an output, a search
/// path, a module or PCH file, a dump or report, the action, the
/// toolchain's directories. Every field of `Args` is named here (no rest
/// pattern), so that a field Kiln adds is classified when the pin moves.
#[allow(clippy::too_many_lines, reason = "one line per field of `Args`")]
fn io_change(base: &Args, a: &Args) -> Option<&'static str> {
    let Args {
        argv: _,
        program: _,
        mode,
        inputs,
        dump_ast,
        dump_ast_filter,
        dump_record_layouts,
        verify,
        dump_vtable_layouts,
        dump_cfg,
        cfg_stats,
        memory_report,
        no_crash_diagnostics,
        crash_diagnostics_dir,
        crash_for_testing,
        permissive: _,
        range_for_ext_temps: _,
        contract_semantic: _,
        contract_checks: _,
        cfg_max_elems: _,
        outline_expansions: _,
        output,
        action,
        std,
        ansi,
        version,
        verbose,
        help,
        print_resource_dir,
        probe,
        print,
        macros,
        dirs,
        frameworks,
        time_trace,
        embed_dirs,
        includes,
        pch,
        macro_prefix_map,
        module_files,
        nameless_module_files,
        precompile,
        phase_args,
        module_header,
        header_unit_main,
        module_output,
        prebuilt_module_paths,
        modules_cache_path,
        module_cache,
        new_bound_check: _,
        rtti_local_marker: _,
        check_new: _,
        elide_constructors: _,
        modules_prune_interval,
        modules_prune_after,
        scan_deps,
        nostdinc,
        nostdincxx,
        nostdlibinc,
        nobuiltininc,
        sysroot,
        isysroot,
        resource_dir,
        gcc_toolchain,
        gcc_install_dir,
        llvm_dir,
        stdlib: _,
        experimental_library: _,
        target,
        diag_args,
        deps,
        predef: _,
        trigraphs: _,
        dollars: _,
        reproducible_time: _,
        source_path_style: _,
        reflection_mangling: _,
        backend,
        hod,
        bmi_kind,
        bmi_image,
        bmi_image_census,
        bmi_image_census_all,
        errors: _,
        constexpr,
        templ,
        codegen: _,
        fp: _,
        sanitize,
        coverage,
        debug: _,
        compile,
        link,
        asm,
        opt_flag: _,
        disable_llvm_passes: _,
        print_commands,
        quiet_unused_arguments: _,
        no_unused: _,
        sanitize_merge_written: _,
        outline_atomics: _,
        march: _,
        mcpu: _,
        fixed_x18: _,
        general_regs_only: _,
        sve_vector_bits: _,
        target_features: _,
        x86_feature_flags: _,
        omit_frame_pointer: _,
        omit_leaf_frame_pointer: _,
        strict_aliasing: _,
        lax_vector_conversions: _,
        auto_var_init: _,
        auto_var_init_max_size: _,
        visibility: _,
        visibility_ms_compat: _,
        invalid_visibility: _,
        visibility_inlines_hidden: _,
        semantic_interposition: _,
        direct_access_external_data: _,
        no_builtin_names: _,
        dump_written_refs,
        decl_extents,
        dump_decl_extents,
        pp_record,
        dump_pp_record,
        host_call_test_handler,
        host_calls_visible,
        env,
        no_side_outputs,
        written_refs,
        dump_expr_extents,
        dump_pp_record_source_order,
        working_directory,
        work_dir,
        written_ref_keys,
        fn_names,
        dump_fn_names,
        dump_friend_forms,
        builtin_literals,
        imacros,
        iprefix,
        prefixed_dirs,
        verify_subst_failed,
        static_init_facts,
    } = a;
    macro_rules! io {
        ($($field:ident),* $(,)?) => {
            $(if *$field != base.$field {
                return Some(stringify!($field));
            })*
        };
    }
    io!(
        mode,
        inputs,
        dump_ast,
        dump_ast_filter,
        dump_record_layouts,
        verify,
        dump_vtable_layouts,
        dump_cfg,
        cfg_stats,
        memory_report,
        no_crash_diagnostics,
        crash_diagnostics_dir,
        crash_for_testing,
        output,
        action,
        std,
        ansi,
        version,
        verbose,
        help,
        print_resource_dir,
        probe,
        print,
        macros,
        dirs,
        frameworks,
        time_trace,
        embed_dirs,
        includes,
        pch,
        macro_prefix_map,
        module_files,
        nameless_module_files,
        precompile,
        phase_args,
        module_header,
        header_unit_main,
        module_output,
        prebuilt_module_paths,
        modules_cache_path,
        module_cache,
        modules_prune_interval,
        modules_prune_after,
        scan_deps,
        nostdinc,
        nostdincxx,
        nostdlibinc,
        nobuiltininc,
        sysroot,
        isysroot,
        resource_dir,
        gcc_toolchain,
        gcc_install_dir,
        llvm_dir,
        target,
        deps,
        backend,
        hod,
        bmi_kind,
        bmi_image,
        bmi_image_census,
        bmi_image_census_all,
        coverage,
        compile,
        link,
        asm,
        print_commands,
        // A dump, a table recorded for a tooling client (the preprocessing
        // record, `-fkiln-pp-record`, `-fkiln-dump-pp-record`: Kiln WP-2110),
        // Kiln's test host-call handler; the last three are no command-line
        // option (`Args::parse_as` leaves them at their defaults).
        dump_written_refs,
        decl_extents,
        dump_decl_extents,
        pp_record,
        dump_pp_record,
        host_call_test_handler,
        host_calls_visible,
        env,
        no_side_outputs,
        // The written-reference table recorded without a dump
        // (`-fkiln-written-refs`, Kiln WP-2320), two dumps
        // (`-fkiln-dump-expr-extents`, Kiln WP-2327;
        // `-fkiln-dump-pp-record=source-order`, Kiln WP-2112), and the
        // directory relative paths open against (`-working-directory`,
        // Kiln WP-2121; `work_dir` is no command-line option, it is
        // `-working-directory` resolved).
        written_refs,
        dump_expr_extents,
        dump_pp_record_source_order,
        working_directory,
        work_dir,
        // Kiln 60fb5ab9d: two tables recorded for a tooling client (the
        // substitution keys, `-fkiln-written-ref-keys`, WP-3540; the
        // function-name spans, `-fkiln-fn-names`, WP-3280), the builtin-macro
        // literal table (`-fkiln-builtin-literals`, WP-2821), two dumps
        // (`-fkiln-dump-fn-names`, `-fkiln-dump-friend-forms`, WP-3164), a
        // report of the silent-failure marks (`-fkiln-verify-subst-failed`,
        // WP-3281), and files and search paths read (`-imacros`,
        // `-iprefix`, `-iwithprefix`/`-iwithprefixbefore`, WP-2822).
        written_ref_keys,
        fn_names,
        dump_fn_names,
        dump_friend_forms,
        builtin_literals,
        verify_subst_failed,
        imacros,
        iprefix,
        prefixed_dirs,
        // Kiln 040bc36bf: static-initialization facts recorded for a tooling
        // client (`-fkiln-static-init-facts`, WP-3650).
        static_init_facts,
    );
    // The parts of three option groups that are files or reports; their
    // limits and the sanitizer set are the language's.
    let format = |args: &[String]| DiagOptions::from_args(args).format;
    let checks: [(bool, &'static str); 10] = [
        // `-fdiagnostics-format=json`: `Parsed::diagnostics` is text.
        (format(diag_args) != format(&base.diag_args), "diag_args (-fdiagnostics-format=)"),
        (constexpr.report != base.constexpr.report, "constexpr.report"),
        (constexpr.stats != base.constexpr.stats, "constexpr.stats"),
        (constexpr.profile != base.constexpr.profile, "constexpr.profile"),
        (constexpr.profile_limit != base.constexpr.profile_limit, "constexpr.profile_limit"),
        (templ.report != base.templ.report, "templ.report"),
        (templ.stack_size != base.templ.stack_size, "templ.stack_size"),
        (sanitize.ignorelists != base.sanitize.ignorelists, "sanitize.ignorelists"),
        (
            sanitize.no_default_ignorelists != base.sanitize.no_default_ignorelists,
            "sanitize.no_default_ignorelists",
        ),
        (sanitize.coverage_lists != base.sanitize.coverage_lists, "sanitize.coverage_lists"),
    ];
    checks.into_iter().find_map(|(changed, field)| changed.then_some(field))
}

/// The parse, on the front end's thread.
fn front_end(
    snippet: &Snippet<'_>,
    checked: &Checked<'_>,
    lang: &Language,
    tables: bool,
) -> Result<Done, String> {
    let sm = SourceManager::new();
    sm.set_macro_prefix_map(&lang.args.macro_prefix_map);
    sm.set_source_path_style(lang.args.source_path_style);
    let interner = Arc::new(Interner::new());
    let buffer = SharedBuffer::new();
    let diag_opts = lang.diag_options();
    let target = lang.sema_target()?;
    let mut popts = lang.parse_options();
    popts.written_refs = tables;
    popts.decl_extents = tables;
    let limits = lang.args.constexpr.limits();
    let features = lang.features()?;

    let (main_path, main_text) = snippet.files[checked.main];
    let main_file = sm
        .add_virtual(main_path, main_text.to_owned())
        .map_err(|_| "the source manager's location space is exhausted".to_owned())?;
    let loaded = std::rc::Rc::new(std::cell::RefCell::new(vec![(main_file, checked.main)]));

    let (tu, lex, errors, warnings) = {
        let engine = DiagEngine::new(&sm, &diag_opts, TextSink::new(buffer.clone(), &diag_opts));
        let mut pp = Preprocessor::new(&sm, &engine, &interner, lang.pp_options());
        configure(&mut pp, snippet, checked, lang, &loaded, features);
        pp.enter_main_file(main_file);
        let (tu, pp) =
            parse_preprocessed(pp, Arc::clone(&interner), &engine, target, &popts, limits);
        let lex = *pp.lex_options();
        drop(pp);
        engine.flush();
        (tu, lex, engine.error_count(), engine.warning_count())
    };
    let diagnostics = buffer.contents();
    let loaded = loaded.borrow().clone();
    Ok(Done {
        ast: tu.ast,
        oven: tu.oven,
        sm,
        main_file,
        errors,
        warnings,
        diagnostics,
        loaded,
        interner,
        lex,
    })
}

/// `#if`'s integer model for the target (`Job::configure_preprocessor`).
fn if_evaluator(lang: &Language) -> IfEval {
    let mut ifeval = IfEval::for_triple(lang.target.triple);
    if let Some(signed) = lang.args.predef.char_signed {
        let mut params = ifeval.params();
        params.char_signed = signed;
        ifeval = IfEval::new(params);
    }
    ifeval
}

/// Finds headers among the snippet's files ([`Snippet::files`]). Each file
/// is added to the source manager the first time it is found, and found as
/// the same buffer after that (so `#pragma once` sees one file).
struct Resolver<'s, 'c> {
    snippet: &'s Snippet<'s>,
    index: &'c HashMap<&'s str, usize>,
    loaded: std::rc::Rc<std::cell::RefCell<Vec<(FileId, usize)>>>,
}

impl Resolver<'_, '_> {
    /// The snippet file `path` names, added to `sm` if it is not yet.
    fn load(&self, sm: &SourceManager, path: &str) -> Option<FileId> {
        let &index = self.index.get(path)?;
        let mut loaded = self.loaded.borrow_mut();
        if let Some(&(file, _)) = loaded.iter().find(|&&(_, i)| i == index) {
            return Some(file);
        }
        let (name, text) = self.snippet.files[index];
        let file = sm.add_virtual(name, text.to_owned()).ok()?;
        loaded.push((file, index));
        Some(file)
    }

    /// The snippet path of a buffer this resolver (or the main file) added.
    fn path_of(&self, file: FileId) -> Option<&str> {
        let loaded = self.loaded.borrow();
        let &(_, index) = loaded.iter().find(|&&(f, _)| f == file)?;
        Some(self.snippet.files[index].0)
    }
}

impl IncludeResolver for Resolver<'_, '_> {
    fn resolve(&mut self, sm: &SourceManager, req: &IncludeRequest) -> Option<ResolvedInclude> {
        let found = |file, lookup_index| ResolvedInclude {
            file,
            lookup_index,
            system: false,
            extern_c: false,
            path: None,
        };
        let next = matches!(req.kind, IncludeKind::IncludeNext | IncludeKind::HasIncludeNext);
        let start = match req.lookup_from {
            Some(i) if next => i as usize + 1,
            _ => 0,
        };
        let beside = !req.angled && start == 0 && !(next && req.lookup_from.is_some());
        if beside && let Some(includer) = req.includer.and_then(|f| self.path_of(f)) {
            let dir = dir_of(includer).to_owned();
            if let Some(file) = join(&dir, &req.name).and_then(|p| self.load(sm, &p)) {
                return Some(found(file, None));
            }
        }
        for (i, dir) in self.snippet.include.iter().enumerate().skip(start) {
            if let Some(file) = join(dir, &req.name).and_then(|p| self.load(sm, &p)) {
                return Some(found(file, Some(i as u32)));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins() {
        assert_eq!(join("", "a.h").as_deref(), Some("a.h"));
        assert_eq!(join("x/y", "../a.h").as_deref(), Some("x/a.h"));
        assert_eq!(join("x", "./b/./a.h").as_deref(), Some("x/b/a.h"));
        assert_eq!(join("", "../a.h"), None);
        assert_eq!(join("x", "/a.h"), None);
        assert_eq!(dir_of("a.h"), "");
        assert_eq!(dir_of("x/y/a.h"), "x/y");
    }

    /// Kiln's `tests/stress/stack-guard-parens.cpp`: 50000 nested
    /// parentheses under `-fkiln-stack-size=8M` are the stack guard's
    /// fatal diagnostic, not a stack overflow. At `kiln`'s default 1 GiB a
    /// debug build needs far more nesting than a test can parse in time
    /// (50000 levels take a minute and do not reach it), so this runs
    /// through `parse_on` at the stress test's size: what it checks is
    /// that the guard is installed on the front end's thread for the size
    /// that thread really has, which is the same code at any size.
    #[test]
    fn stack_guard() {
        let depth = 50_000;
        let mut text = String::from("int before = 1;\nint x = ");
        text.extend(std::iter::repeat_n('(', depth));
        text.push('0');
        text.extend(std::iter::repeat_n(')', depth));
        text.push_str(";\nint after = 2;\n");
        let files = [("deep.cpp", text.as_str())];
        let s = Snippet { files: &files, main: "deep.cpp", include: &[], std: "c++20" };
        let (errors, diags) = parse_on(&s, &Extra::default(), Some(8 << 20), false, |p| {
            (p.errors, p.diagnostics.clone())
        })
        .unwrap();
        assert!(errors >= 1);
        assert!(
            diags.starts_with("deep.cpp:2:")
                && diags.contains(": fatal error: kiln: stack exhausted during parsing;"),
            "{}",
            &diags[..diags.len().min(400)]
        );
    }

    /// A one-file snippet `main.cpp` in `std`.
    fn one<'a>(files: &'a [(&'a str, &'a str)], std: &'a str) -> Snippet<'a> {
        Snippet { files, main: "main.cpp", include: &[], std }
    }

    /// `(errors, diagnostics)` of `text` in `std` under `args` and `target`.
    fn run(
        text: &str,
        std: &str,
        args: &[&str],
        target: Option<&str>,
    ) -> Result<(u32, String), String> {
        let files = [("main.cpp", text)];
        parse_with(&one(&files, std), &Extra { args, target }, |p| {
            (p.errors, p.diagnostics.clone())
        })
    }

    /// `kiln -std=<std> [--target=] <args> -fsyntax-only -x c++ -` on
    /// `text`: its standard error without the summary line, the input
    /// named `main.cpp` (as `tests/snippet.rs` does it for WP-021).
    fn kiln_stderr(std: &str, args: &[&str], target: Option<&str>, text: &str) -> String {
        kiln_run(std, args, target, text).1
    }

    /// [`kiln_stderr`] with `kiln`'s exit status first.
    fn kiln_run(std: &str, args: &[&str], target: Option<&str>, text: &str) -> (i32, String) {
        let err = SharedBuffer::new();
        let io = kiln_driver::Io {
            stdout: Box::new(std::io::sink()),
            stderr: kiln_driver::SharedWriter::new(err.clone()),
            stdin: Box::new(std::io::Cursor::new(text.as_bytes().to_vec())),
        };
        // The host is not the default target: `kiln` is told it.
        let mut line =
            vec![format!("-std={std}"), format!("--target={}", target.unwrap_or(TRIPLE))];
        line.extend(args.iter().map(|a| (*a).to_owned()));
        line.extend(["-fsyntax-only", "-x", "c++", "-"].map(str::to_owned));
        let status = kiln_driver::run_as("kiln", &line, io);
        let text = err
            .contents()
            .replace("<stdin>", "main.cpp")
            .lines()
            .filter(|l| !l.ends_with(" generated."))
            .flat_map(|l| [l, "\n"])
            .collect();
        (status, text)
    }

    // WP-038 "Rules of `parse_with`": `parse` is `parse_with` with
    // `Extra::default()`, and a target given as the default triple is the
    // default.
    #[test]
    fn wp038_default_extra_is_parse() {
        let text = "int f() { return missing; }\nstatic_assert(sizeof(long) == 8);\n";
        let files = [("main.cpp", text)];
        let plain = parse(&one(&files, "c++20"), |p| p.diagnostics.clone()).unwrap();
        let with = run(text, "c++20", &[], None).unwrap().1;
        let named = run(text, "c++20", &[], Some("x86_64-unknown-linux-gnu")).unwrap().1;
        assert_eq!(plain, with);
        assert_eq!(plain, named);
        assert!(plain.contains("missing"), "{plain}");
    }

    /// Arguments `parse_with` refuses alone, by spelling or by what they
    /// read or write, or because the driver reports an error for them.
    const REFUSED: &[&str] = &[
        "-o",
        "-oout.o",
        "-Iinc",
        "-iquoteinc",
        "-isystem/usr/include",
        "-idirafter/x",
        "--sysroot=/",
        "-resource-dir=/x",
        "-include",
        "-includeh.h",
        "--include=h.h",
        "-imacros",
        "-DX=1",
        "-UX",
        "-x",
        "-xc",
        "-c",
        "-S",
        "-E",
        "-emit-llvm",
        "-M",
        "-MM",
        "-MD",
        "-MMD",
        "-MFdeps.d",
        "-MTx",
        "-fmodule-file=m=m.pcm",
        "-fmodule-file=m.pcm",
        "-fmodule-output",
        "-fmodule-output=m.pcm",
        "-fmodule-header",
        "-fprebuilt-module-path=d",
        "-fmodules-cache-path=d",
        "--precompile",
        "-verify",
        "-verify=x",
        "-std=c++17",
        "-std=c++20",
        "--std=c++20",
        "-ansi",
        "a.cpp",
        "-",
        "--target=x86_64-unknown-linux-gnu",
        "-target",
        "-march=native",
        "-mcpu=native",
        "-mtune=native",
        "-fkiln-stack-size=8M",
        "-fdump-ast",
        "-fdump-record-layouts",
        "-fkiln-dump-cfg",
        "-fkiln-memory-report",
        // The preprocessing record (Kiln WP-2110, WP-101).
        "-fkiln-pp-record",
        "-fkiln-dump-pp-record",
        // Kiln WP-2320, WP-2327, WP-2112, WP-2121 (WP-066).
        "-fkiln-written-refs",
        "-fkiln-dump-expr-extents",
        "-fkiln-dump-pp-record=source-order",
        "-working-directory=/",
        "-ftime-trace",
        "-fconstexpr-report=text",
        "-fkiln-instantiation-report=text",
        "-fsanitize-ignorelist=list.txt",
        "-fprofile-instr-generate",
        "--coverage",
        "-include-pch",
        "-emit-pch",
        "--driver-mode=g++",
        "-fkiln-backend=hod",
        "-v",
        "-###",
        "--help",
        "-lm",
        "-Wl,-z,now",
        "-fbogus",
        "-fms-extensions",
        "-fdeclspec",
        "-fconstexpr-steps=lots",
        "-target-feature",
        "-Xclang",
        "-Xlinker",
        "-mllvm",
        "-fsyntax-only",
        "-Xpreprocessor",
        "-fdiagnostics-format=json",
    ];

    // WP-038: an argument that changes what is read or written, one the
    // driver does not know or reports an error for, and one that makes the
    // result depend on the host, each `Err` naming it before any parse.
    #[test]
    fn wp038_refused_arguments() {
        let refused = REFUSED;
        let text = "int x;\n";
        for arg in refused {
            let err = run(text, "c++20", &[arg], None).expect_err(arg);
            assert!(err.contains(&format!("`{arg}`")), "{arg}: {err}");
        }
    }

    // WP-038: arguments that change only the language, accepted alone and
    // together on one line.
    #[test]
    fn wp038_accepted_arguments() {
        let text = "int x;\n";
        let accepted = [
            "-fno-rtti",
            "-fno-exceptions",
            "-funsigned-char",
            "-frounding-math",
            "-ffp-contract=off",
            "-O2",
            "-g",
            "-fPIC",
            "-Wall",
            "-Werror",
            "-fkiln-reproducible-time",
            "-fsanitize=address",
        ];
        for arg in accepted {
            assert_eq!(run(text, "c++20", &[arg], None).map(|r| r.0), Ok(0), "{arg}");
        }
        assert_eq!(run(text, "c++20", &accepted, None).map(|r| r.0), Ok(0));
    }

    // WP-038 "Rules of `parse_with`", the net under them: on a source
    // whose job always reports (an undeclared name; a shift too wide for
    // the `-W` options to act on), `parse_with`
    // is `Err` exactly when `kiln` on the same line stops before the job
    // (a failing status and no diagnostic of the job's), and the job's
    // diagnostics are `kiln`'s when both run (the driver's own, `kiln: `,
    // are not in `Parsed`). `REFUSED` is not run by `kiln`: some of it
    // writes files.
    #[test]
    fn wp038_err_exactly_when_kiln_stops() {
        let text = "int x = 1 << 40;\nint y = undeclared;\n";
        let arm = Some("aarch64-unknown-linux-gnu");
        let stops: [(&[&str], Option<&str>); 15] = [
            (&["-fgnu89-inline"], None),
            (&["-fvisibility=bogus"], None),
            (&["-Werror", "-mno-outline-atomics"], None),
            (&["-Werror=option-ignored", "-moutline-atomics"], None),
            (&["-Werror", "-ffp-model=fast", "-ffp-contract=on"], None),
            (&["-Werror", "-mlarge-data-threshold=10"], None),
            (&["-Werror", "-m3dnow"], None),
            (&["-Werror", "-msse4.2"], arm),
            (&["-fdiagnostics-format=sarif"], None),
            (&["-ferror-limit=x"], None),
            (&["-fbogus"], None),
            (&["-fno-rtti", "-fsanitize=vptr"], None),
            (&["-msve-vector-bits=512"], None),
            (&["-march=armv9-a+sve2"], None),
            (&["-fconstexpr-steps=lots"], None),
        ];
        let runs: [(&[&str], Option<&str>); 16] = [
            (&[], None),
            (&["-mno-outline-atomics"], None),
            (&["-Werror", "-Wno-option-ignored", "-mno-outline-atomics"], None),
            (&["-Werror", "-Wno-error=option-ignored", "-mno-outline-atomics"], None),
            (&["-ffp-model=fast", "-ffp-contract=on"], None),
            (&["-Werror"], None),
            (&["-w", "-Werror", "-mno-outline-atomics"], None),
            (&["-Wall", "-fno-rtti", "-funsigned-char"], None),
            (&["-fno-caret-diagnostics", "-fno-show-column"], None),
            (&["-fno-diagnostics-show-line-numbers"], None),
            (&["-fdiagnostics-format=clang", "-ftabstop=0"], None),
            (&["-fvisibility=hidden", "-fno-gnu89-inline"], None),
            (&["-O2", "-g", "-fsanitize=address"], None),
            (&["-march=armv8-a+sve", "-Werror"], arm),
            (&["-msse4.2"], arm),
            (&["-Werror", "-mno-outline-atomics"], arm),
        ];
        for (args, target) in stops {
            let (status, theirs) = kiln_run("c++20", args, target, text);
            assert!(status != 0 && !theirs.contains("main.cpp:"), "{args:?}: kiln ran: {theirs}");
            let ours = run(text, "c++20", args, target);
            assert!(ours.is_err(), "{args:?} {target:?}: accepted, kiln stops: {theirs}");
        }
        for (args, target) in runs {
            let (_, theirs) = kiln_run("c++20", args, target, text);
            let job: String = theirs
                .lines()
                .filter(|l| !l.starts_with("kiln: "))
                .flat_map(|l| [l, "\n"])
                .collect();
            assert!(job.contains("main.cpp:"), "{args:?} {target:?}: kiln stopped: {theirs}");
            let ours = run(text, "c++20", args, target);
            assert_eq!(ours.map(|r| r.1), Ok(job), "{args:?} {target:?}");
        }
        for arg in REFUSED {
            assert!(run(text, "c++20", &[arg], None).is_err(), "{arg}");
        }
    }

    // WP-038: an error only the target or the arguments together give.
    #[test]
    fn wp038_errors_of_target_and_combination() {
        let text = "int x;\n";
        let sve = ["-msve-vector-bits=512"];
        let err = run(text, "c++20", &sve, None).unwrap_err();
        assert!(err.starts_with("argument `-msve-vector-bits=512`"), "{err}");
        assert_eq!(run(text, "c++20", &sve, Some("aarch64-linux-gnu")).map(|r| r.0), Ok(0));
        let march = ["-march=armv9-a+sve2"];
        assert!(run(text, "c++20", &march, None).is_err());
        assert_eq!(run(text, "c++20", &march, Some("aarch64-linux-gnu")).map(|r| r.0), Ok(0));
        // Each alone is fine; together the driver rejects them.
        let both = ["-fno-rtti", "-fsanitize=vptr"];
        assert!(run(text, "c++20", &both[..1], None).is_ok());
        assert!(run(text, "c++20", &both[1..], None).is_ok());
        let err = run(text, "c++20", &both, None).unwrap_err();
        assert!(err.starts_with("arguments `-fno-rtti -fsanitize=vptr` together"), "{err}");
    }

    // WP-038: the triples `DriverTarget::parse` knows, and two it does
    // not; the same source under the two targets (`va_list`'s size and
    // `char`'s sign are the target's).
    #[test]
    fn wp038_targets() {
        let x86 = "static_assert(sizeof(__builtin_va_list) == 24);\n\
                   static_assert(char(-1) < 0);\n\
                   static_assert(alignof(long double) == 16);\n";
        let arm = "static_assert(sizeof(__builtin_va_list) == 32);\n\
                   static_assert(char(-1) > 0);\n\
                   static_assert(alignof(long double) == 16);\n";
        for t in ["x86_64", "x86_64-linux-gnu", "x86_64-pc-linux-gnu", "amd64-unknown-linux"] {
            assert_eq!(run(x86, "c++20", &[], Some(t)).map(|r| r.0), Ok(0), "{t}");
            assert!(run(arm, "c++20", &[], Some(t)).unwrap().0 > 0, "{t}");
        }
        for t in ["aarch64", "aarch64-unknown-linux-gnu", "aarch64-linux-gnu", "arm64-linux"] {
            assert_eq!(run(arm, "c++20", &[], Some(t)).map(|r| r.0), Ok(0), "{t}");
            assert!(run(x86, "c++20", &[], Some(t)).unwrap().0 > 0, "{t}");
        }
        assert_eq!(run(x86, "c++20", &[], None).map(|r| r.0), Ok(0));
        for t in ["riscv64-unknown-linux-gnu", "x86_64-apple-darwin", "i686-linux-gnu", ""] {
            let err = run(x86, "c++20", &[], Some(t)).unwrap_err();
            assert!(err.starts_with(&format!("unknown target `{t}`")), "{err}");
        }
    }

    // WP-038: one construct per flag family that is an error without the
    // flag (or the target) and parses with it. `__declspec` has none:
    // Kiln's driver knows neither `-fms-extensions` nor `-fdeclspec`
    // (refused above) and its parser rejects the keyword
    // (`kiln_parse/src/attr.rs:626`).
    #[test]
    fn wp038_flag_families() {
        // (source, std, the arguments and target without, with).
        type Side<'a> = (&'a [&'a str], Option<&'a str>);
        let arm = Some("aarch64-unknown-linux-gnu");
        let cases: [(&str, &str, Side<'_>, Side<'_>); 6] = [
            ("__SVInt8_t *p;\n", "c++20", (&[], None), (&[], arm)),
            // `^^` is lexed only in C++26 with reflection on
            // (`kiln_lex/src/lexer.rs:1567`), its default there.
            ("constexpr auto r = ^^int;\n", "c++26", (&["-fno-reflection"], None), (&[], None)),
            ("int f(int x) pre(x > 0);\n", "c++23", (&[], None), (&["-fcontracts"], None)),
            ("int f(int x) pre(x > 0);\n", "c++26", (&["-fno-contracts"], None), (&[], None)),
            (
                "struct V { virtual ~V(); };\nstruct W : V {};\nW *g(V *v) { return dynamic_cast<W *>(v); }\n",
                "c++20",
                (&["-fno-rtti"], None),
                (&["-fno-rtti", "-frtti"], None),
            ),
            ("void g() { throw 1; }\n", "c++20", (&["-fno-exceptions"], None), (&[], None)),
        ];
        for (text, std, (off, off_target), (on, on_target)) in cases {
            let bad = run(text, std, off, off_target).unwrap();
            assert!(bad.0 > 0, "{text} {off:?}: {}", bad.1);
            assert_eq!(run(text, std, on, on_target).unwrap(), (0, String::new()), "{text} {on:?}");
        }
        let decl = "__declspec(noinline) void f();\n";
        assert!(run(decl, "c++20", &[], None).unwrap().0 > 0);
    }

    // WP-038 "Rules of `parse_with`": the fields `Language` sets by hand
    // follow the arguments: `complex_promoted`, the target features and
    // `-ffp-eval-method=`'s `__FLT_EVAL_METHOD__`.
    #[test]
    fn wp038_fields_set_by_hand() {
        let lang = |args: &[&str], target| Language::new("c++20", &Extra { args, target }).unwrap();
        assert!(!lang(&[], None).parse_options().complex_promoted);
        assert!(lang(&["-fcomplex-arithmetic=promoted"], None).parse_options().complex_promoted);
        assert!(!lang(&["-fcomplex-arithmetic=basic"], None).parse_options().complex_promoted);
        let arm = lang(&["-march=armv8-a+sve"], Some("aarch64-linux-gnu")).parse_options();
        assert!(arm.sve && arm.x86_features.is_empty() && !arm.target_features.is_empty());
        let x86 = lang(&["-mavx2"], None).parse_options();
        assert!(!x86.sve && x86.target_features.is_empty());
        assert!(x86.x86_features.iter().any(|f| f == "+avx2"), "{:?}", x86.x86_features);
        assert!(lang(&["-frounding-math"], None).parse_options().fp.rounding_math);
        let eval = "#if __FLT_EVAL_METHOD__ != 1\n#error not double\n#endif\n";
        assert_eq!(run(eval, "c++20", &["-ffp-eval-method=double"], None).map(|r| r.0), Ok(0));
        assert!(run(eval, "c++20", &[], None).unwrap().0 > 0);
        let asan = "#if !__has_feature(address_sanitizer)\n#error no asan\n#endif\n";
        assert_eq!(run(asan, "c++20", &["-fsanitize=address"], None).map(|r| r.0), Ok(0));
        let neon = "#if !__has_builtin(__builtin_neon_vaddq_v)\n#error no neon\n#endif\n";
        assert_eq!(run(neon, "c++20", &[], Some("aarch64-linux-gnu")).map(|r| r.0), Ok(0));
        assert!(run(neon, "c++20", &[], None).unwrap().0 > 0);
    }

    // WP-038: on sources that use no predefined macro and no header, the
    // diagnostics under extra arguments and a target are `kiln`'s, text
    // for text.
    #[test]
    fn wp038_same_diagnostics_as_kiln() {
        let cases: [(&str, &[&str], Option<&str>, &str); 10] = [
            ("c++20", &["-fno-rtti"], None, "struct V { virtual ~V(); };\nauto &t = typeid(V);\n"),
            ("c++20", &["-fno-exceptions"], None, "void g() { try { throw 1; } catch (int) {} }\n"),
            (
                "c++20",
                &["-Werror"],
                None,
                "int h() { int u; return u; }\nint k(int a) { return a; return 1; }\n",
            ),
            ("c++20", &[], Some("aarch64-unknown-linux-gnu"), "__SVInt8_t v;\n__SVInt8_t *p;\n"),
            ("c++20", &[], None, "__SVInt8_t *p;\n"),
            (
                "c++20",
                &["-msve-vector-bits=256"],
                Some("aarch64-linux-gnu"),
                "typedef __SVInt32_t f __attribute__((arm_sve_vector_bits(256)));\nstatic_assert(sizeof(f) == 32);\n",
            ),
            ("c++23", &["-fcontracts"], None, "int f(int x) pre(x > 0) post(r: r > 0);\n"),
            ("c++26", &["-fno-reflection"], None, "constexpr auto r = ^^int;\n"),
            (
                "c++26",
                &[],
                Some("aarch64"),
                "constexpr auto r = ^^int;\nstatic_assert(char(-1) < 0);\n",
            ),
            ("c++20", &["-funsigned-char"], None, "static_assert(char(-1) < 0);\n"),
        ];
        for (std, args, target, text) in cases {
            let ours = run(text, std, args, target).unwrap().1;
            let theirs = kiln_stderr(std, args, target, text);
            assert_eq!(ours, theirs, "{std} {args:?} {target:?}: {text}");
        }
    }

    // WP-021's sixteen threads under an `Extra`: every result is that of
    // the snippet parsed alone, on any thread.
    #[test]
    fn wp038_threads() {
        const THREADS: usize = 16;
        const ROUNDS: usize = 20;
        let texts: Vec<String> = (0..THREADS)
            .map(|i| {
                format!(
                    "__SVInt8_t *p{i};\nstatic_assert(char(-1) > 0);\n\
                     int f{i}(int x) pre(x > {i});\nint bad_{i} = undeclared_{i};\n"
                )
            })
            .collect();
        let extra = Extra { args: &["-fcontracts", "-frounding-math"], target: Some("aarch64") };
        let digest = |text: &str| {
            let files = [("main.cpp", text)];
            parse_with(&one(&files, "c++23"), &extra, |p| {
                let n = p.ast.members(p.ast.tu).count();
                format!("{n} {} {}", p.errors, p.diagnostics)
            })
            .unwrap()
        };
        let alone: Vec<String> = texts.iter().map(|t| digest(t)).collect();
        for (i, a) in alone.iter().enumerate() {
            assert!(a.contains(&format!("undeclared_{i}")) && !a.contains("SVInt8"), "{a}");
        }
        std::thread::scope(|scope| {
            for i in 0..THREADS {
                let (texts, alone, digest) = (&texts, &alone, &digest);
                scope.spawn(move || {
                    for round in 0..ROUNDS {
                        assert_eq!(digest(&texts[i]), alone[i], "thread {i}, round {round}");
                    }
                });
            }
        });
    }

    #[test]
    fn normalised_paths() {
        assert!(normalised("a/b.h"));
        for bad in ["", "/a", "a/", "a//b", "./a", "a/../b"] {
            assert!(!normalised(bad), "{bad:?}");
        }
    }
}
