#include "side.h"
// WP-163 fix round 2 (N5), a known false mismatch pinned: It<T, (n::k)> against It<T, n::k> names the same constant, but rule 12 adds the written expression's profile, whose bytes hold the parentheses, so M::p changes. The loud direction, and parity with the profile of the pin before WP-163.
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <class T, int I> struct It {};
#ifdef B
template <class T> struct M { It<T, n::k>* p; };
#else
template <class T> struct M { It<T, (n::k)>* p; };
#endif
