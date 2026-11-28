#include "side.h"
// WP-156 fix round 2 (X2), fix round 3 (moved from `indeterminate`: `kiln/destructor-name` is deleted): a destructor named through a template-id with an operand. `make(x)` binds to `lib::make` on side A and to `::make<lib::X>` on side B; its entry is a site-9 entry of `dtor`'s operand unit (L3, L5).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
Wr<int> wv;
void dtor() { wv.~Wr<decltype(make(x))>(); }
