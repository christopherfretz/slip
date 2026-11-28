#include "side.h"
// WP-156 fix round 1 (W3, W10), fix round 2 (M-2), fix round 3 (moved from `indeterminate`: `kiln/placeholder-constraint` is deleted): a declaration whose type a constrained placeholder deduced, before its name, in a trailing return type and in a `new`. `make(x)` binds to `lib::make` on side A and to `::make<lib::X>` on side B; its entry is a site-9 entry of the unit that holds it (L3, L5); the concept names are written by T4. `auto w`, the lambda, `new int(4)` and `k` write nothing.
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T, class U> concept Same = __is_same(T, U);
Same<decltype(make(x))> auto v = 1;
Same<decltype(make(x))> auto g() { return 1; }
auto t() -> Same<decltype(make(x))> auto { return 2; }
auto w = 2;
int h() { return [] { return 3; }(); }
int *n = new int(4);
int *n2 = new Same<decltype(make(x))> auto(4);
int k = 5;
// A class declared by an elaborated-type-specifier inside the
// type-constraint's arguments.
template <class T, class U, class V, class W> concept Same3 = __is_same(T, U);
Same3<decltype(make(x)), int, struct Z *> auto z3 = 1;
