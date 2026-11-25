#include "side.h"
// WP-156 fix round 2 (X3): an explicit instantiation that writes its own
// `noexcept(e)`. Kiln keeps the specialisation (its exception
// specification substituted from the pattern), not the directive's
// declarator: one whose function type, up to its exception specification,
// is that of a declarator no declaration links and that holds an operand
// is indeterminate (`kiln/explicit-instantiation-written`).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
template <class T> void g1(T) noexcept(true) {}
template void g1<int>(int) noexcept(sizeof(make(x)) == 4);
template <class T> void g3(T) noexcept(true) {}
extern template void g3<long>(long) noexcept(sizeof(make(x)) == 4);
void u() { g3(1L); }
template <class T> struct G6 { void f() noexcept(true) {} };
template void G6<int>::f() noexcept(sizeof(make(x)) == 4);
