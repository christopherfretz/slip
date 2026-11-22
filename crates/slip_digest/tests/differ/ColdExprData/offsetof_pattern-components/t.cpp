#include "side.h"
// §7.4 ColdExprData: written by `expr_misc::offsetof_components`
#ifdef B
template <class T> struct A { unsigned long w = __builtin_offsetof(T, m); };
#else
template <class T> struct A { unsigned long w = __builtin_offsetof(T, n); };
#endif
