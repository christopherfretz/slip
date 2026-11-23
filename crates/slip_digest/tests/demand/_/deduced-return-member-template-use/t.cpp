#include "side.h"
// digest §14 (WP-045 review probe): a deduced return type of a member
// function template of an implicit instantiation.
template <class T> struct X { template <class U> auto f(U u) { return u; } };
X<int> x;
#ifdef B
long use() { return x.f(1L); }
#endif
