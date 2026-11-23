#include "side.h"
// §7 "Parentheses" (F-B): the same for a variable template's initialiser.
constexpr int x = 1;
#ifdef B
template<class T> constexpr decltype(auto) v = (x);
#else
template<class T> constexpr decltype(auto) v = x;
#endif
