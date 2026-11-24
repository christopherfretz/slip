#include "side.h"
// §7.4 ColdExprData: the operand type alone differs: one base, one member, one offset
struct Bs { int x; }; struct D : Bs {}; struct D2 : Bs {};
#ifdef B
unsigned long v = __builtin_offsetof(D2, x);
#else
unsigned long v = __builtin_offsetof(D, x);
#endif
