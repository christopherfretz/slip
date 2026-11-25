#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 (B4): the holders of a written type in an expression, each with
// an operand whose binding moves (`lib::make` against `::make<lib::X>`).
namespace std { class type_info; }
template <int N> struct Q { static constexpr int v = N; template <int M> static int t() { return M; } };
struct C { template <int M> int m() { return M; } };
int e_alignof = alignof(decltype(make(x)));
const std::type_info &e_typeid = typeid(decltype(make(x)));
bool e_trait = __is_same(decltype(make(x)), int);
int *e_new = new decltype(make(x));
int e_fcast = decltype(make(x))(0);
int e_value = decltype(make(x))();
int e_qual = Q<sizeof(make(x))>::v;
int e_qual_t = Q<1>::t<sizeof(make(x))>();
int e_member(C c) { return c.template m<sizeof(make(x))>(); }
int e_compound = ((Q<sizeof(make(x))>){}).v;
// A friend's written type (a declaration's holder), and a substituted
// parameter's replacement, where the walk stops (`Wr<…>::m` has no operand).
template <class T, unsigned long N> struct arr {};
struct Fr2 { friend struct arr<char, sizeof(make(x))>; };
template <class T> struct Wr { T m; };
Wr<decltype(make(x))> wr;
// A named cast, `sizeof` of a type, and a compound literal of array type
// (Kiln keeps its written type, `compound_literal_written`), in a body:
// a file-scope one is unsupported (`CompoundLiteralExpr.aux`).
long e_scast = static_cast<decltype(make(x))>(0L);
unsigned long e_sizeof = sizeof(decltype(make(x)));
int e_carr() { return ((Q<sizeof(make(x))>[]){{}})[0].v; }
// A cast to a reference alias: the written type is the alias (no
// operand: the alias's own unit holds it), not the node's type.
using Rr = decltype(make(x)) &;
int e_rcast(int &i) { return static_cast<Rr>(i); }
// Explicit template arguments of an overloaded member.
struct C2 { template <int M> int m() { return M; } int m(long) { return 0; } };
int e_movl(C2 c) { return c.m<sizeof(make(x))>(); }
