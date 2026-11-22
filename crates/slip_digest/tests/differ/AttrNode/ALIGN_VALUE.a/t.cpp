#include "side.h"
// §10.1 AttrNode: `ALIGN_VALUE.a`: the fact differs.
#ifdef B
void f(int *p __attribute__((align_value(32))));
#else
void f(int *p __attribute__((align_value(16))));
#endif
