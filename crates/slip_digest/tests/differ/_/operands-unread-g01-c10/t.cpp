#include "side.h"
// WP-156 fix round 3 (L3, L5): `make(x)` binds to `lib::make` on side A and to `::make<lib::X>` on side B, in a place no stream walks; the written name is a site-9 entry of the unit that holds it.
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
template <class T, class U> concept Same = __is_same(T, U);
void d10() { Wr<int> wv{0}; wv.~decltype(Wr<int>{make(x)})(); }
