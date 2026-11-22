#include "side.h"
// digest §2.2 (WP-032 rule 1): `g` is declared at namespace scope, used,
// and defined as a friend of a class template. On side B the class is
// instantiated, so the use instantiates the friend's definition: it is
// no definition of `g`'s declared unit (rule 4: the resolved unit's).
void g(int);
template <class T> struct A { friend void g(int) {} };
#ifdef B
void inst() { A<int> a; }
#else

#endif
void use() { g(1); }
