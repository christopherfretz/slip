#include "side.h"
// digest §14 (WP-045 review probe): the overload ADL finds at the point
// of instantiation of S<N::A> chooses its base class.
namespace N { struct A {}; struct B1 {}; struct B2 {}; B1 g(A, int); }
#ifdef B
namespace N { B2 g(A, long); }
#endif
template <class T> struct S : decltype(g(T(), 0L)) {};
S<N::A> s;
#ifndef B
namespace N { B2 g(A, long); }
#endif
