#include "side.h"
// §10.1 AttrKind: `ALIGN_VALUE`: the attribute on one side only.
#ifdef B
void f(int *p __attribute__((align_value(16))));
#else
void f(int *p);
#endif
