#include "side.h"
// digest-kinds §11.2 (WP-046 review F1): forming the pointer resolves the
// specification on both sides, so an earlier use that resolves it first
// changes no declared unit, P included.
struct M { M &operator=(const M &) noexcept(false); };
struct S { M m; S &operator=(const S &) = default; };
#ifdef B
bool h(S &a) { return noexcept(a = a); }
#endif
using P = decltype(&S::operator=);
