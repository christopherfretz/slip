//! Temporary exploration (deleted before commit).

use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::source::Span;
use slip_kiln::snippet::{Extra, Snippet, parse_with_tables};

fn dump(src: &str) {
    let files = [("main.cpp", src)];
    let snippet = Snippet { files: &files, main: "main.cpp", include: &[""], std: "c++23" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        eprintln!("--- errors {}: {}", p.errors, p.diagnostics);
        let unit = p.unit();
        let ast = p.ast;
        let sm = p.sm;
        for r in slip_kiln::written::written_refs(unit) {
            let text = sm.span_text(r.name).unwrap_or("?");
            let d = ast.decls.get(r.referent);
            let ident = match ast.names.view(d.name()) {
                DeclNameView::Identifier(s) => ast.symbols().as_str(s).to_owned(),
                _ => "<not ident>".to_owned(),
            };
            let fr = slip_kiln::ranges::file_range_of_span(sm, r.name).map(|f| {
                format!("{:?} {:?}", &sm.text(f.file)[f.lo as usize..f.hi as usize], f.tier)
            });
            let ext = slip_kiln::ranges::file_range_of_span(sm, Span::new(r.first, r.name.hi))
                .map(|f| format!("{:?}", &sm.text(f.file)[f.lo as usize..f.hi as usize]));
            eprintln!(
                "{:?} {:?} -> {:?} {:?} global={} using={} macro={} range={:?} ext={:?}",
                r.role,
                text,
                d.kind(),
                ident,
                r.global,
                r.via_using,
                r.name.lo.is_macro(),
                fr,
                ext
            );
        }
        for (d, row) in ast.decl_extents.iter() {
            let q = if row.qual.is_valid() {
                sm.span_text(Span::new(row.qual, row.name.hi)).map(str::to_owned)
            } else {
                None
            };
            eprintln!(
                "row {:?} {:?} qual={:?} full={:?}",
                ast.decls.get(d).kind(),
                sm.span_text(row.name),
                q,
                sm.span_text(row.full)
            );
        }
        for n in slip_kiln::written::dependent_names(unit) {
            eprintln!("dep {:?} {:?} unresolved={}", n.role, sm.span_text(n.name), n.unresolved);
        }
    })
    .unwrap();
}

#[test]
fn explore2() {
    dump(
        "namespace a { namespace b { struct T { void f(); static int s; }; template <class U> void tf(U); } }
void a::b::T::f() {}
int a::b::T::s = 1;
using a::b::T;
using namespace a::b;
namespace Y = a::b;
template <> void a::b::tf<int>(int) {}
template void a::b::tf<long>(long);
template <class C> concept Co = true;
static_assert(Co<int>);
struct P { int x, y; }; P pp = { .x = 1, .y = 2 };
struct B0 {}; struct D0 : B0 { using Base = B0; D0() : Base() {} };
struct M { int k = sizeof(T); M() {} M(int) {} };
void q() { a::b::template tf<int>(1); a::b::tf(1); a::b::tf(1.0); }
struct S2; struct S2 *ps; struct W { friend struct S2; };
",
    );
}

#[test]
fn explore() {
    dump(
        "namespace a { namespace b { int c; struct T { int m; void f(); ~T(); operator int(); }; T operator+(T, T); } }
namespace X = a::b;
typedef a::b::T TT;
using a::b::c;
int u1 = a::b::c + ::a::b::c + :: /*x*/ a::b::c + X::c + c;
void g(TT t, a::b::T *p) { t.m = 1; p->f(); p->~T(); (void)t.operator int(); (void)operator+(t, t); t + t; p->a::b::T::f(); }
struct D : a::b::T { int k; D() : a::b::T(), k(2) {} };
template <class T> struct V { T v; template <class U> void tm(); };
V<int> vv;
template <class T> void h(T t) { t.m = 0; f(t); typename T::x y; V<T>::template tm<int>(); }
#define ID(x) x
#define TWICE(x) x + x
#define CALL a::b::c
int u2 = ID(c) + TWICE(c) + CALL;
enum E { e1 }; int u3 = e1 + E::e1;
void k() { int q = 1; auto l = [q] { return q; }; (void)l; }
",
    );
}

#[test]
fn explore3() {
    dump(
        "namespace n { struct S {}; }\nstruct n::S *p;\nint c;\n#define REV(x, y) y x + 0\nnamespace a { int c; }\nint u = REV(c, a::);\n",
    );
}
