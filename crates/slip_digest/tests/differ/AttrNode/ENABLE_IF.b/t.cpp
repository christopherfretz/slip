#include "side.h"
// §10.1 AttrNode: `ENABLE_IF.b`: the fact differs.
#ifdef B
void f(int x) __attribute__((enable_if(x > 0, "n")));
#else
void f(int x) __attribute__((enable_if(x > 0, "m")));
#endif
