#include "side.h"
// WP-156 fix round 2 (X3), fix round 3 (moved from `indeterminate`: `kiln/explicit-instantiation-written` is deleted): an explicit instantiation that writes its own `noexcept(e)`. `make(x)` binds to `lib::make` on side A and to `::make<lib::X>` on side B; its entry is a site-9 entry of the unit that holds it (L3, L5; the member's: the class the qualifier enters, KT1 2.9 (a)).
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
