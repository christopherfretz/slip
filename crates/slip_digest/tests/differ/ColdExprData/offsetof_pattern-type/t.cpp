#include "side.h"
// §7.4 ColdExprData `offsetof_pattern-type`.
template<class T, class U> struct A {
#ifdef B
static inline unsigned long w = __builtin_offsetof(U, m); };
#else
static inline unsigned long w = __builtin_offsetof(T, m); };
#endif
