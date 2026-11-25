#include "side.h"
// WP-155 fix round (B2, rule 4): a class with a constexpr constructor,
// initialised by it. clang emits nothing for Tr<int>::r (its
// initialisation is constant); Kiln does not evaluate it and Slip cannot
// ask it to (spec/landed/verify/WP-155.md §1), so it stays marked: a known
// mark too many, on Kiln's list.
struct P { constexpr P(int v) : v(v) {} int v; };
template <class T> struct Tr { static P r; };
template <class T> P Tr<T>::r{1};
#ifndef B
P *pp = &Tr<int>::r;
#endif
