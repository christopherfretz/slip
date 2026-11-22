#include "side.h"
// digest §14 demand independence: a friend function template defined in a
// class template and declared first at namespace scope; instantiating the
// class sets `TemplateTables` `instantiated_from` on the first declaration
// (kiln_sema/src/templ/member.rs:316-329), which changes `g`'s declared unit.
template <class U> void g(U);
template <class T> struct A { template <class U> friend void g(U) {} };
#ifdef B
void use() { A<int> a; }
#else

#endif
