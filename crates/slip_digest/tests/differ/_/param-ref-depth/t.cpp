#include "side.h"
// §4.4 profile mode, item 6: `a` is the outer function's parameter, `b` the inner one's (function-scope depth).
#ifdef B
template<class T> auto f(T a, auto (*g)(T b) -> decltype(b + b)) -> int;
#else
template<class T> auto f(T a, auto (*g)(T b) -> decltype(a + b)) -> int;
#endif
