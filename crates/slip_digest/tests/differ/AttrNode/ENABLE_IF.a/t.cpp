#include "side.h"
// §10.1 AttrNode: `ENABLE_IF.a`: the fact differs.
#ifdef B
void f(int x) __attribute__((enable_if(x > 1, "")));
#else
void f(int x) __attribute__((enable_if(x > 0, "")));
#endif
