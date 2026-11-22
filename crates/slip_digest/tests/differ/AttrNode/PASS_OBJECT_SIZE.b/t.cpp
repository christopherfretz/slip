#include "side.h"
// §10.1 AttrNode: `PASS_OBJECT_SIZE.b`: the fact differs.
#ifdef B
void f(void *p __attribute__((pass_dynamic_object_size(0))));
#else
void f(void *p __attribute__((pass_object_size(0))));
#endif
