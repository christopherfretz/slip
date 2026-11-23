// std: c++26
#include "side.h"
// §6 ExpansionData `expansions`: the instantiation for a class with another member count: the pattern parts are the same.
#ifdef B
struct P { int x, y, z; };
template <class T> void f(T t) { template for (int e : t) ; }
template void f<P>(P);
#else
struct P { int x, y; };
template <class T> void f(T t) { template for (int e : t) ; }
template void f<P>(P);
#endif
