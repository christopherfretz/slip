#include "side.h"
// WP-163 fix round 3: a partial specialisation of a variadic template, whose canonical list holds its pack converted (`T, {0}`), two values, only the second differing (n::k against n::j, both 0).
namespace n { constexpr int k = 0; constexpr int j = 0; constexpr int m = 0; }
#ifdef B
template <class, int...> struct P {};
template <class T> struct P<T, n::m, n::j> { int m; };
#else
template <class, int...> struct P {};
template <class T> struct P<T, n::m, n::k> { int m; };
#endif
