#include "side.h"
// digest §14 demand independence: a function template declared at
// namespace scope and defined as a friend of a class template, then used.
// On side B the use instantiates `t<int>` from the friend's definition,
// and Kiln gives the specialisation the definition's `FnSpec` (inline, as
// a function defined in its class is); on side A `t` has no definition.
template <class U> void t(U);
template <class T> struct A { template <class U> friend void t(U) {} };
#ifdef B
void make() { A<int> a; }
#else

#endif
void use() { t(1); }
