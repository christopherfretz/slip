#include "side.h"
// WP-163 fix round 3: a partial specialisation of a variadic template, whose canonical list holds its pack converted (`T, {0}`), a value then a written pack expansion (`n::k, Ns...`): the pack has two elements, the expansion pairs with its own, and n::k is read against 0.
namespace n { constexpr int k = 0; constexpr int j = 0; constexpr int m = 0; }
#ifdef B
template <class, int...> struct P {};
template <class T, int... Ns> struct P<T, n::j, Ns...> { int m; };
#else
template <class, int...> struct P {};
template <class T, int... Ns> struct P<T, n::k, Ns...> { int m; };
#endif
