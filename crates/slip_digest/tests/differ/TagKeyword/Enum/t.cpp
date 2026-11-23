// std: c++20
#include "side.h"
// §4.3 TagKeyword `Enum`: `enum T::x` against `typename T::x`.
template <class T> struct A {
#ifdef B
using X = typename T::x;
#else
using X = enum T::x;
#endif
};
