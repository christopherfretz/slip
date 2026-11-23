// std: c++26
#include "side.h"
// digest-kinds rule §3.3 `field`: the same members in an instantiation.
template <class T> struct S {
#ifdef B
T _; long _;
#else
T _; int _;
#endif
};
S<char> s;
