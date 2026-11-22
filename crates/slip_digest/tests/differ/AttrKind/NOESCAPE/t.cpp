#include "side.h"
// §10.1 AttrKind: `NOESCAPE`: the attribute on one side only.
#ifdef B
void f(int *p __attribute__((noescape)));
#else
void f(int *p);
#endif
