#include "side.h"
// WP-163 fix round 3: a partial specialisation of a variadic template, whose canonical list holds its pack converted (`T, {0}`), an out-of-line member through P<T, n::k> (n::j on side B, also flagged): the canonical 0 is inside the pack, which `holds_value` walks, so the unit is indeterminate under spelling/converted-argument (own_spelling `out_of_line_member`).
namespace n { constexpr int k = 0; constexpr int j = 0; constexpr int m = 0; }
#ifdef B
template <class, int...> struct P { void f(); };
template <class T> struct P<T, 0> { void f(); };
template <class T> void P<T, n::j>::f() {}
#else
template <class, int...> struct P { void f(); };
template <class T> struct P<T, 0> { void f(); };
template <class T> void P<T, n::k>::f() {}
#endif
