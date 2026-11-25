#include "side.h"
// WP-156 (B5, S8): `void_t<int>` against `void` in ordinary code: a site-8
// operand unit on side B only, and no finding.
template <class...> using void_t = void;
#ifdef B
void_t<int> f();
#else
void f();
#endif
