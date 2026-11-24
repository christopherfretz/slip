#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123, form B): the argument of a befriended specialisation in a class template, which Kiln keeps as written in the canonical type (not profiled), names another constant. Written by its profile.
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <class T, int I> struct It;
#ifdef B
template <class T> struct M { friend struct It<T, n::j>; };
#else
template <class T> struct M { friend struct It<T, n::k>; };
#endif
