// std: c++20
#include "side.h"
// §5.4 FnSpec `0-not-selected`: of two prospective destructors, the one overload resolution does not select.
#ifdef B
template <class T> struct X { ~X() requires (sizeof(T) == 8); ~X(); };
X<int> x;
#else
template <class T> struct X { ~X() requires (sizeof(T) == 4); ~X(); };
X<int> x;
#endif
