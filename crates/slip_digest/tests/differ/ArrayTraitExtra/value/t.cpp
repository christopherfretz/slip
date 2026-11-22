#include "side.h"
// §7 ArrayTraitExtra: `value`: the fact differs.
#ifdef B
typedef int I[1][2];
unsigned long v = __array_rank(I);
#else
typedef int I[1];
unsigned long v = __array_rank(I);
#endif
