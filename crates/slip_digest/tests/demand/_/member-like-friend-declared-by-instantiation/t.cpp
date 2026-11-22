#include "side.h"
// digest §14 demand independence: a member-like friend (its constraint
// names the class's parameter) is declared by each instantiation of the
// class; today a declared unit `g` exists only on side B.
template <class T> concept C = true;
template <class T> struct A { template <class U> friend void g(U) requires C<T> {} };
#ifdef B
A<int> a;
#else

#endif
