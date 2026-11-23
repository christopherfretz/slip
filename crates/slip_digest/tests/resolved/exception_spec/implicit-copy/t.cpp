#include "side.h"
// digest-kinds §11.2 rule 1: an implicitly declared copy constructor,
// defined (and its specification resolved) by k's copy on both sides; a
// second use changes nothing.
struct M { M(const M &) noexcept(false); M(); };
struct S { M m; };
void k(const S &s) { S t(s); }
#ifdef B
bool h(const S &s) { return noexcept(S(s)); }
#endif
