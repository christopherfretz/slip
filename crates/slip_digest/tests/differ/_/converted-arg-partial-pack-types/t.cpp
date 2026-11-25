#include "side.h"
// WP-163 fix round 3: a partial specialisation of a variadic template, whose canonical list holds its pack converted (`T, {0}`), the control: a type pack holds no converted value, and the unit stays determinate (its member's initialiser differs).
namespace n { constexpr int k = 0; constexpr int j = 0; constexpr int m = 0; }
#ifdef B
template <class, class...> struct P {};
template <class T> struct P<T, int, long> { int m = n::j; };
#else
template <class, class...> struct P {};
template <class T> struct P<T, int, long> { int m = n::k; };
#endif
