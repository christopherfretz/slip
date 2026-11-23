#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123, form B): the same constant named through a namespace alias in a befriended specialisation's argument. The profile writes the constant by key, and a namespace's qualifier holds nothing more (rule 11).
namespace n { constexpr int k = 0; }
namespace m = n;
template <class T, int I> struct It;
#ifdef B
template <class T> struct M { friend struct It<T, m::k>; };
#else
template <class T> struct M { friend struct It<T, n::k>; };
#endif
