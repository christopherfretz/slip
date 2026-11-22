#include "side.h"
// §7 OffsetofStep: `Index.elem_size`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
typedef long E;
struct S { E x[4]; };
unsigned long v = __builtin_offsetof(S, x[a]);
#else
typedef int E;
struct S { E x[4]; };
unsigned long v = __builtin_offsetof(S, x[a]);
#endif
