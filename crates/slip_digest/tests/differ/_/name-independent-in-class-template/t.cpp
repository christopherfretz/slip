// std: c++26
#include "side.h"
// digest-kinds rule §3.3 `field`: two `_` members of a class template, distinct and determinate.
template <class T> struct S {
#ifdef B
T _; long _;
#else
T _; int _;
#endif
};
