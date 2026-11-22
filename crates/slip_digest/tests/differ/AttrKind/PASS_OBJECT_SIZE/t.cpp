#include "side.h"
// §10.1 AttrKind: `PASS_OBJECT_SIZE`: the attribute on one side only.
#ifdef B
void f(void *p __attribute__((pass_object_size(0))));
#else
void f(void *p);
#endif
