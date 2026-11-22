#include "side.h"
// §10.1 AttrNode: `CALLBACK.a`: the fact differs.
#ifdef B
void f(void (*cb)(int, int), int x, int y) __attribute__((callback(cb, y, x)));
#else
void f(void (*cb)(int, int), int x, int y) __attribute__((callback(cb, x, y)));
#endif
