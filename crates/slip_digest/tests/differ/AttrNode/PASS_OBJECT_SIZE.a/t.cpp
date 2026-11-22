#include "side.h"
// §10.1 AttrNode: `PASS_OBJECT_SIZE.a`: the fact differs.
#ifdef B
void f(void *p __attribute__((pass_object_size(1))));
#else
void f(void *p __attribute__((pass_object_size(0))));
#endif
