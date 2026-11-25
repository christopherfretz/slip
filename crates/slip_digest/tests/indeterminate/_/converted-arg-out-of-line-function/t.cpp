#include "side.h"
// WP-163 fix round 2 (B2): a member function of the partial specialisation P<T, 0> defined out of line through P<T, n::k> against P<T, n::j>. Kiln keeps no qualifier of an out-of-line declaration, so the written argument cannot be read: the unit is indeterminate under spelling/converted-argument, never equal, with n::j as with n::k (side B, seen by the differ run) (own_spelling `out_of_line_member`).
namespace n { constexpr int k = 0; constexpr int j = 0; }
#ifdef B
template <class T, int I> struct P { void f(); };
template <class T> struct P<T, 0> { void f(); };
template <class T> void P<T, n::j>::f() {}
#else
template <class T, int I> struct P { void f(); };
template <class T> struct P<T, 0> { void f(); };
template <class T> void P<T, n::k>::f() {}
#endif
