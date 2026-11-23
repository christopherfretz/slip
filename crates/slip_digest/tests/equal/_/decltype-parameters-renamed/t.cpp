#include "side.h"
// §4.4, §7 _ `decltype-parameters-renamed`: the parameters of a profile renamed: `Ref::param` by position.
#ifdef B
template<class U> auto f(U b, U a) -> decltype(b + a);
#else
template<class T> auto f(T a, T b) -> decltype(a + b);
#endif
