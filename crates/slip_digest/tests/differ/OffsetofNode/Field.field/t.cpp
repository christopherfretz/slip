#include "side.h"
// §7 OffsetofNode: `Field.field`: the fact differs.
union U { int a; int b; };
#ifdef B
unsigned long v = __builtin_offsetof(U, b);
#else
unsigned long v = __builtin_offsetof(U, a);
#endif
