#include "side.h"
// §7.4 ColdExprData `offsetof_pattern-components`.
// written by `expr_misc::offsetof_components`
template<class T> struct A {
#ifdef B
static inline unsigned long w = __builtin_offsetof(T, n); };
#else
static inline unsigned long w = __builtin_offsetof(T, m); };
#endif
