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
//! Those of `kiln -std=<std> -fsyntax-only` with no other flag, computed by
//! Kiln's public functions from Kiln's own parse of that command line
//! (`kiln_driver::args::Args`), for the target `x86_64-unknown-linux-gnu`.
//! Where they differ:
//!
//! - **No predefined macros**: the predefines buffer is empty (`kiln` reads
//!   it from its resource directory), so `__cplusplus`, `__x86_64__`,
//!   `__GNUC__` and the rest are not defined. The builtin macros the
//!   preprocessor defines itself (`__FILE__`, `__LINE__`, `__COUNTER__`,
//!   `__has_include`, …) are.
//! - **No header search**: includes resolve only among the snippet's files
//!   (see [`Snippet::files`]); no file is ever system, and `#embed` finds
//!   nothing.
//! - **Modules**: from C++20 the preprocessor has
//!   `kiln_pp::ModuleOptions::default()`, as `kiln_parse::parse_tu` does:
//!   no module file, no prebuilt path, no `std` module directory or cache
//!   (those are found on the file system), so an `import` is "module not
//!   found".
//! - **`__has_builtin`** answers from `kiln_pp::ClangFeatures` for the
//!   target, without the driver's staging of the builtins Kiln does not
//!   handle yet (`StagedFeatures` is private to `kiln_driver`); the other
//!   feature queries are the driver's.
//! - **The note after a missing `<meta>`** at C++26 (`PpOptions::meta_hint`)
//!   is not given: it depends on the GCC installation on the host.
//! - **`__DATE__`, `__TIME__` and `__TIMESTAMP__`** are those of the Unix
//!   epoch, as under `kiln -fkiln-reproducible-time`: a snippet parses to
//!   the same translation unit whenever and wherever it is parsed.
//! - **`ParseOptions::complex_promoted`** is `false`, the value with no
//!   flag; the driver compares a `kiln_codegen` type to compute it, and this
//!   crate does not link `kiln_codegen`.
//! - The options that only module, PCH or header-unit output sets
//!   (`record_carried`, `record_image`, `prefix`, `header_unit`) are
//!   `false`, as with no flag.
//!
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

use kiln_diag::{DiagEngine, DiagOptions, SharedBuffer, TextSink};
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
use kiln_source::{FileId, SourceManager};

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

impl Parsed<'_> {
    /// The snippet path a buffer was added under; `None` for a buffer the
    /// front end made itself.
    pub fn path(&self, file: FileId) -> Option<&str> {
        self.paths.get(&file).copied()
    }
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
pub fn parse<R>(snippet: &Snippet<'_>, f: impl FnOnce(&Parsed<'_>) -> R) -> Result<R, String> {
    parse_on(snippet, None, f)
}

/// [`parse`] with the front end's thread given `stack` bytes instead of
/// `kiln`'s default (`-fkiln-stack-size=`; the tests of the stack guard).
fn parse_on<R>(
    snippet: &Snippet<'_>,
    stack: Option<usize>,
    f: impl FnOnce(&Parsed<'_>) -> R,
) -> Result<R, String> {
    let checked = check(snippet)?;
    let mut lang = Language::new(snippet.std)?;
    if let Some(bytes) = stack {
        lang.line.push(format!("-fkiln-stack-size={bytes}"));
    }
    let size = kiln_driver::templ_opts::stack_size_of(&lang.line);
    let done = std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("kiln".into())
            .stack_size(size)
            .spawn_scoped(scope, || {
                // As the `kiln` executable's compile thread does.
                kiln_sema::stack_guard::StackGuard::install(size);
                front_end(snippet, &checked, &lang)
            })
            .map_err(|e| {
                format!("cannot start the front end's thread with a {size}-byte stack: {e}")
            })?;
        handle.join().map_err(|payload| format!("the front end panicked: {}", text_of(&*payload)))
    })??;
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
    };
    catch_unwind(AssertUnwindSafe(|| f(&parsed)))
        .map_err(|payload| format!("the callback panicked: {}", text_of(&*payload)))
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
}

/// The language options of `kiln -std=<std> -fsyntax-only` with no other
/// flag, as far as Kiln's public functions give them.
struct Language {
    /// The command line they come from (without the program name).
    line: Vec<String>,
    args: Args,
    std: LangStd,
    target: DriverTarget,
}

impl Language {
    fn new(std: &str) -> Result<Language, String> {
        let lang = STDS.contains(&std).then(|| LangStd::parse(std)).flatten();
        let Some(lang) = lang else {
            return Err(format!("unknown std `{std}`; one of {}", STDS.join(", ")));
        };
        let line = vec![format!("-std={std}"), "-fsyntax-only".to_owned()];
        let args = Args::parse_as("kiln", &line);
        let Some(target) = DriverTarget::parse(TRIPLE) else {
            unreachable!("Kiln's driver knows the triple {TRIPLE}");
        };
        Ok(Language { line, args, std: lang, target })
    }

    fn x86(&self) -> bool {
        !self.target.is_aarch64()
    }

    /// The diagnostic options (`run_as`).
    fn diag_options(&self) -> DiagOptions {
        let mut o = DiagOptions::from_args(&self.args.diag_args);
        o.clang_compat = self.args.predef.clang_compat.major();
        o
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
            // `a.complex_range() == ComplexRange::Promoted`; the type is
            // `kiln_codegen`'s, and with no flag it is `Full`.
            complex_promoted: false,
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
        let mut fargs: Vec<String> = a.argv.clone();
        fargs.extend(a.sanitize(x86, triple).0.feature_args());
        fargs.push(if self.std.gnu { "-std=gnu++" } else { "-std=c++" }.to_owned());
        let features = ClangFeatures::from_args(triple, self.std.lex, &fargs)
            .ok_or_else(|| format!("Kiln has no feature tables for {triple}"))?
            .with_compat(a.predef.clang_compat);
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

/// The parse, on the front end's thread.
fn front_end(
    snippet: &Snippet<'_>,
    checked: &Checked<'_>,
    lang: &Language,
) -> Result<Done, String> {
    let sm = SourceManager::new();
    sm.set_macro_prefix_map(&lang.args.macro_prefix_map);
    sm.set_source_path_style(lang.args.source_path_style);
    let interner = Arc::new(Interner::new());
    let buffer = SharedBuffer::new();
    let diag_opts = lang.diag_options();
    let target = lang.sema_target()?;
    let popts = lang.parse_options();
    let limits = lang.args.constexpr.limits();
    let features = lang.features()?;

    let (main_path, main_text) = snippet.files[checked.main];
    let main_file = sm
        .add_virtual(main_path, main_text.to_owned())
        .map_err(|_| "the source manager's location space is exhausted".to_owned())?;
    let loaded = std::rc::Rc::new(std::cell::RefCell::new(vec![(main_file, checked.main)]));

    let (tu, errors, warnings) = {
        let engine = DiagEngine::new(&sm, &diag_opts, TextSink::new(buffer.clone(), &diag_opts));
        let mut pp = Preprocessor::new(&sm, &engine, &interner, lang.pp_options());
        pp.set_include_resolver(Box::new(Resolver {
            snippet,
            index: &checked.index,
            loaded: std::rc::Rc::clone(&loaded),
        }));
        pp.set_if_evaluator(Box::new(if_evaluator(lang)));
        pp.set_feature_queries(Box::new(features));
        pp.enter_main_file(main_file);
        let (tu, pp) =
            parse_preprocessed(pp, Arc::clone(&interner), &engine, target, &popts, limits);
        drop(pp);
        engine.flush();
        (tu, engine.error_count(), engine.warning_count())
    };
    let diagnostics = buffer.contents();
    let loaded = loaded.borrow().clone();
    Ok(Done { ast: tu.ast, oven: tu.oven, sm, main_file, errors, warnings, diagnostics, loaded })
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
        let (errors, diags) =
            parse_on(&s, Some(8 << 20), |p| (p.errors, p.diagnostics.clone())).unwrap();
        assert!(errors >= 1);
        assert!(
            diags.starts_with("deep.cpp:2:")
                && diags.contains(": fatal error: kiln: stack exhausted during parsing;"),
            "{}",
            &diags[..diags.len().min(400)]
        );
    }

    #[test]
    fn normalised_paths() {
        assert!(normalised("a/b.h"));
        for bad in ["", "/a", "a/", "a//b", "./a", "a/../b"] {
            assert!(!normalised(bad), "{bad:?}");
        }
    }
}
