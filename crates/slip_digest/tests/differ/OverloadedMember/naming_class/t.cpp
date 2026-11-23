#include "side.h"
// §7.4 OverloadedMember `naming_class`: the class access is checked in.
// `p->f(x<T>)` through a derived class that inherits the overloads,
// against the base: the same candidates, a different naming class.
struct Bn { int f(int); int f(long); };
struct Dn : Bn {};
#ifdef B
Bn *p;
#else
Dn *p;
#endif
template <class T> T x{};
template <class T> auto v = p->f(x<T>);
