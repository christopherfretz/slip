#include "side.h"
// digest §2.2, WP-032 rule 1: a friend function template the instantiation
// declares first, defined later at namespace scope and called: the unit is
// read from the written definition, whose `DeclMisc` friend kind is that of
// a written template (a specialisation copies its template's).
template <class T> struct A { template <class U> friend int g(U); };
#ifdef B
void use() { A<int> a; }
#endif
template <class U> int g(U) { return 0; }
int u = g(1);
