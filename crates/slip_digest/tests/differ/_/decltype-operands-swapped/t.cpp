#include "side.h"
// §4.4, §7 _ `decltype-operands-swapped`: `decltype(a + b)` against `decltype(b + a)`: parameters by position.
#ifdef B
template<class T> auto f(T a, T b) -> decltype(a + b);
template<class T> auto f(T a, T b) -> decltype(b + a);
#else
template<class T> auto f(T a, T b) -> decltype(a + b);
#endif
