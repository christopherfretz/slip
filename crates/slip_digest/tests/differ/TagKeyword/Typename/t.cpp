// std: c++20
#include "side.h"
// §4.3 TagKeyword `Typename`: `typename T::x` against `struct T::x`.
template <class T> struct A {
#ifdef B
using X = struct T::x;
#else
using X = typename T::x;
#endif
};
