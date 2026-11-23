// std: c++20
#include "side.h"
// §4.3 TagKeyword `Union`: `union T::x` against `enum T::x`.
template <class T> struct A {
#ifdef B
using X = enum T::x;
#else
using X = union T::x;
#endif
};
