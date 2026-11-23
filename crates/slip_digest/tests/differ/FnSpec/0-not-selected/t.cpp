// std: c++20
#include "side.h"
// §5.4 FnSpec `0-not-selected`: of two prospective destructors, the one overload resolution does not select.
// The constraint names `c<T>`, so the destructors' keys stay (their
// trailing requires-clause is in their discriminator, §3.3).
#ifdef B
template <class T> constexpr bool c = sizeof(T) == 8;
#else
template <class T> constexpr bool c = sizeof(T) == 4;
#endif
template <class T> struct X { ~X() requires c<T>; ~X(); };
X<int> x;
