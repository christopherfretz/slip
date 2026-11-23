//! Digests one observer of a plan and prints what made units
//! indeterminate (WP-123, "How to find the cases"): for finding the
//! corpus's forms one translation unit at a time, never a corpus run.
//!
//! ```text
//! SLIP_PLAN=<plan file> SLIP_ONLY=<substring of the main file> \
//!   cargo run -p slip_cli --example digest_one -j 1
//! ```
//!
//! Prints, for the first observer whose main file's path contains
//! `SLIP_ONLY`: the parse's status; every `digest.*` counter of the walk;
//! then each indeterminate unit, `cause<TAB>name`, sorted. With
//! `SLIP_CAUSE=<substring>` only the units whose cause contains it. With
//! `SLIP_EXPR_ARGS=1`, also every canonical type whose canonical argument
//! list holds an expression without a profile (`ArgView::Expr`, digest-
//! kinds §4.6): the type as Kiln prints it, and each such expression's
//! kind, dependence and place.

use std::process::ExitCode;

use slip_digest::{Detail, Options};
use slip_kiln::kiln::ast::types::{ArgView, TypeView};
use slip_kiln::kiln::ast::{ArgListId, Ast, ExprId, QualType};
use slip_kiln::{ParseOptions, parse_observer_with};
use slip_model::Phase;
use slip_model::Plan;

fn main() -> ExitCode {
    let (Some(plan), Some(only)) = (std::env::var_os("SLIP_PLAN"), std::env::var("SLIP_ONLY").ok())
    else {
        eprintln!("usage: SLIP_PLAN=<plan file> SLIP_ONLY=<substring> digest_one");
        return ExitCode::from(2);
    };
    let cause_filter = std::env::var("SLIP_CAUSE").ok();
    let expr_args = std::env::var_os("SLIP_EXPR_ARGS").is_some();
    let bytes = match std::fs::read(&plan) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}: {e}", plan.to_string_lossy());
            return ExitCode::from(2);
        }
    };
    let plan = match Plan::from_bytes(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("plan: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(observer) = plan
        .observers
        .iter()
        .find(|o| o.main.path.windows(only.len()).any(|w| w == only.as_bytes()))
    else {
        eprintln!("no observer's main file contains {only}");
        return ExitCode::from(1);
    };
    println!("observer {}", String::from_utf8_lossy(&observer.main.path));
    let rules = slip_model::keys::RuleTable::empty();
    let options = Options {
        rules: &rules,
        read_only: slip_digest::ReadOnly::EVERY_FILE,
        detail: Detail::All,
        names: true,
        rule_use: false,
    };
    let identity = |path: &[u8]| path.to_vec();
    let result = parse_observer_with(observer, Phase::Map, 0, ParseOptions::default(), |unit| {
        let types = if expr_args { expr_arg_types(unit.ast(), unit.sm()) } else { Vec::new() };
        (slip_digest::map::map_unit(unit, &options, &identity), types)
    });
    let (shard, walked) = match result {
        Ok(x) => x,
        Err(e) => {
            eprintln!("parse: {e}");
            return ExitCode::from(1);
        }
    };
    println!("status {:?}", shard.outcome.status);
    let Some((walked, types)) = walked else {
        println!("no parse");
        return ExitCode::from(1);
    };
    for t in types {
        println!("expr-arg {t}");
    }
    let output = match walked {
        Some(Ok(o)) => o,
        Some(Err(e)) => {
            eprintln!("walk: {e}");
            return ExitCode::from(1);
        }
        None => {
            println!("the unit has errors: no walk");
            return ExitCode::from(1);
        }
    };
    for c in &output.counters {
        if c.name.starts_with("digest.") {
            println!("counter {} {}", c.name, c.value);
        }
    }
    let names: std::collections::BTreeMap<_, _> = output.names.iter().cloned().collect();
    let mut lines: Vec<String> = output
        .causes
        .iter()
        .map(|(k, c)| format!("{c}\t{}", names.get(k).map_or("?", String::as_str)))
        .filter(|l| {
            cause_filter
                .as_deref()
                .is_none_or(|f| l.split('\t').next().is_some_and(|c| c.contains(f)))
        })
        .collect();
    lines.sort();
    for l in lines {
        println!("unit {l}");
    }
    ExitCode::SUCCESS
}

/// Every canonical type whose canonical argument list holds an
/// expression without a profile, with each such expression.
fn expr_arg_types(ast: &Ast, sm: &slip_kiln::kiln::source::SourceManager) -> Vec<String> {
    let types = &ast.types;
    let mut out = Vec::new();
    for i in 1..=u32::try_from(types.len()).unwrap_or(u32::MAX) {
        let Some(t) = slip_kiln::kiln::ast::TypeId::try_new(i) else { continue };
        if !types.is_canonical(t) {
            continue;
        }
        let (form, lists): (&str, Vec<ArgListId>) = match types.view(t) {
            TypeView::TemplateSpecialization { args, .. } => ("TemplateSpecialization", vec![args]),
            TypeView::DependentTemplateSpecialization { args, .. } => {
                ("DependentTemplateSpecialization", vec![args])
            }
            TypeView::Auto { args: Some(args), .. } => ("Auto", vec![args]),
            TypeView::SubstTemplateTypeParmPack { pack, .. } => {
                ("SubstTemplateTypeParmPack", vec![pack])
            }
            _ => continue,
        };
        let mut exprs = Vec::new();
        let mut stack = lists;
        while let Some(l) = stack.pop() {
            for a in types.args(types.canonical_args(l)) {
                match a.view() {
                    ArgView::Expr { expr, profile: None } => exprs.push(expr),
                    ArgView::Pack(p) => stack.push(p),
                    _ => {}
                }
            }
        }
        if exprs.is_empty() {
            continue;
        }
        let printed = slip_kiln::kiln::ast::types::print_type(
            types,
            ast,
            QualType::unqual(t),
            slip_kiln::kiln::ast::types::PrintOptions::CANONICAL,
        );
        let mut line = format!("{form}\t{printed}");
        for e in exprs {
            line.push_str(&format!("\t[{}]", describe(ast, sm, e)));
        }
        out.push(line);
    }
    out
}

fn describe(ast: &Ast, sm: &slip_kiln::kiln::source::SourceManager, e: ExprId) -> String {
    let n = ast.exprs.get(e);
    let at = sm.line_col(ast.begin_loc(e)).map_or_else(
        || "?".to_owned(),
        |lc| format!("{}:{}:{}", sm.file_name(lc.file), lc.line, lc.col),
    );
    format!(
        "{:?} type-dep={} value-dep={} {at}",
        n.kind(),
        n.is_type_dependent(),
        n.is_value_dependent()
    )
}
