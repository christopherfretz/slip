#include "side.h"
// §7 _: an earlier use instantiates `W<int>`; the trait's unit is unchanged.
template <class T> struct W { T t; W(T); };
#ifdef B
unsigned long w = sizeof(W<int>);
#else

#endif
bool v = __is_constructible(W<int>, int);
