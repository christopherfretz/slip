#include "side.h"
// WP-163 fix round 3: a partial specialisation of a variadic template, whose canonical list holds its pack converted (`T, {0}`), the variable template form, n::k against n::j.
namespace n { constexpr int k = 0; constexpr int j = 0; constexpr int m = 0; }
#ifdef B
template <class, int...> constexpr int v = 0;
template <class T> constexpr int v<T, n::j> = 1;
#else
template <class, int...> constexpr int v = 0;
template <class T> constexpr int v<T, n::k> = 1;
#endif
