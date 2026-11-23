// std: c++20
#include "side.h"
// §4.3 TagKeyword `Struct`: `struct T::x` against `union T::x`.
template <class T> struct A {
#ifdef B
using X = union T::x;
#else
using X = struct T::x;
#endif
};
