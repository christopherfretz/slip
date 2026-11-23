#include "side.h"
// §7 "Parentheses", F2: `&T::m` is a pointer to member, `&(T::m)` is not; the parenthesis of a dependent operand is kept.
#ifdef B
template<class T> auto f(T) -> decltype(&(T::m));
#else
template<class T> auto f(T) -> decltype(&T::m);
#endif
