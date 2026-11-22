#include "side.h"
// digest §2.2 (WP-032 rule 1): a friend naming a member of another class
// redeclares it; the instantiation's declaration is not part of
// `S::f`'s declared unit.
struct S { void f(); };
template <class T> struct A { friend void S::f(); };
#ifdef B
void use() { A<int> a; }
#else

#endif
