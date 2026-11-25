//! TEMPORARY probe (WP-156): removed before the handback.
use slip_kiln::kiln::ast::decl::DeclKind;

thread_local! { pub(crate) static OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
thread_local! { pub(crate) static TRACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[test]
fn probe_lambda_noexcept() {
    let src = "int f() { return [](int a) noexcept([] { return true; }()) { return a; }(1); }\n";
    crate::decl_func::tests::with_walk(src, |w| {
        let ast = w.ast;
        for d in ast.decls.ids() {
            let k = ast.decls.get(d).kind();
            if ast.decls.record(d).is_some_and(|r| r.lambda.is_some()) {
                let o = ast.closure_owner(d);
                let ok = o.map(|o| ast.decls.get(o).kind());
                let res = crate::local::owner_res(w, d);
                eprintln!("PROBE closure {d:?} owner {o:?} {ok:?} res {res:?}");
                if let Ok(o) = res {
                    let n = crate::local::numbering(w, o);
                    eprintln!("PROBE   numbering types {:?}", n.types);
                    let t = crate::local::type_number(w, d);
                    eprintln!("PROBE   type_number {t:?}");
                }
            }
            if k == DeclKind::CXXMethod {
                eprintln!("PROBE method {d:?} local {} deferrable {}", w.is_local(d), crate::demand::deferrable(w, d));
            }
        }
    });
}

#[test]
fn probe_friend_list() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/indeterminate/_/spelling-friend-qualified-class");
    crate::expr_profile::tests::each_input(&dir, |input| {
        TRACE.with(|c| c.set(true));
        OFF.with(|c| c.set(true));
        for no in [false] {
            crate::own_spelling::NO_LIST.with(|c| c.set(no));
            let rules = slip_model::keys::RuleTable::empty();
            let o = crate::digest(
                input,
                &crate::Options {
                    rules: &rules,
                    read_only: crate::ReadOnly::NO_FILE,
                    detail: crate::Detail::All,
                    names: true,
                    rule_use: false,
                },
            );
            for (k, n) in &o.names {
                let c: Vec<_> = o.causes.iter().filter(|(x, _)| x == k).map(|(_, c)| *c).collect();
                eprintln!("PROBE2 no_list={no} {k} {n} {c:?}");
            }
        }
        crate::own_spelling::NO_LIST.with(|c| c.set(false));
    });
}
