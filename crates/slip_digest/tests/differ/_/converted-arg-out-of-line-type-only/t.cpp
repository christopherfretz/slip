#include "side.h"
// WP-163 fix round 2 (B2), the bound: an out-of-line member of a partial specialisation whose canonical arguments hold no converted value stays determinate (its body differs).
namespace n { constexpr int k = 0; constexpr int j = 0; }
#ifdef B
template <class T, class U> struct P { void f(); };
template <class T> struct P<T, int> { void f(); };
template <class T> void P<T, int>::f() { (void)n::j; }
#else
template <class T, class U> struct P { void f(); };
template <class T> struct P<T, int> { void f(); };
template <class T> void P<T, int>::f() { (void)n::k; }
#endif
