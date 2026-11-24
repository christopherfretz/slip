#include "side.h"
// §7.4 ColdExprData: two members of a union: one type, one offset
union U { int a; int b; };
#ifdef B
unsigned long v = __builtin_offsetof(U, b);
#else
unsigned long v = __builtin_offsetof(U, a);
#endif
