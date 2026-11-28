#include "side.h"
// WP-156 fix round 2 (X8), fix round 3 (moved from `indeterminate`: `kiln/bit-int-width` is deleted for a `_BitInt(N)`): `make(x)` in a width binds to `lib::make` on side A and to `::make<lib::X>` on side B; Kiln keeps the width as a number and the walk reads no expression there, so the written name is a site-9 entry (L3, L5) of the declaration's operand unit. `lit` writes nothing. A pattern's `_BitInt(e)` (`bt`, `DependentBitInt`) keeps the cause.
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
_BitInt(sizeof(make(x)) * 8) bi = 0;
unsigned _BitInt(sizeof(make(x)) * 8) bu = 0;
template <class T> _BitInt(sizeof(make(x)) * 8) bt(T) { return 0; }
_BitInt(32) lit = 0;
