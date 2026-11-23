#include "side.h"
// §4.4, §7 _ `decltype-paren`: `decltype(a)` against `decltype((a))`: the outermost parenthesis of a profile is kept.
#ifdef B
template<class T> auto f(T a) -> decltype(a);
template<class T> auto f(T a) -> decltype((a));
#else
template<class T> auto f(T a) -> decltype(a);
#endif
