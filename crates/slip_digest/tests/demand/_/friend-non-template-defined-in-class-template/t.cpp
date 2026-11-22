#include "side.h"
// digest §2.2 (WP-032 rule 1): `g` is declared at namespace scope and
// defined as a friend of a class template. Instantiating the class adds a
// declaration with a body to `g`'s chain; `g`'s declared unit does not move.
void g(int);
template <class T> struct A { friend void g(int) {} };
#ifdef B
void use() { A<int> a; }
#else

#endif
