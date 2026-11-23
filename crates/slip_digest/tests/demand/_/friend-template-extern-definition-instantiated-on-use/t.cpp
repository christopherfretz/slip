#include "side.h"
// digest-kinds §11.9 (WP-045 review, N5): t<int> is `extern` from the
// template's first declaration; `0-implicit-inline`, set when its definition
// is instantiated from the friend's (kiln_sema/src/class/members.rs:906),
// does not make the own unit's storage `extern`.
template <class U> extern void t(U);
template <class T> struct A { template <class U> friend void t(U) {} };
#ifdef B
void make() { A<int> a; }
#endif
void use() { t(1); }
