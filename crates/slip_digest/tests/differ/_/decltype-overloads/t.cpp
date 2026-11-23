#include "side.h"
// §4.4, §7 _ `decltype-overloads`: two functions overloaded on `decltype` of different dependent expressions have different keys.
#ifdef B
template<class T> auto f(T a) -> decltype(a + 1);
template<class T> auto f(T a) -> decltype(a - 1);
#else
template<class T> auto f(T a) -> decltype(a + 1);
#endif
