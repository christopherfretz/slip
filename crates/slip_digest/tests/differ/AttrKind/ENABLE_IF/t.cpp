#include "side.h"
// §10.1 AttrKind: `ENABLE_IF`: the attribute on one side only.
#ifdef B
void f(int x) __attribute__((enable_if(x > 0, "")));
#else
void f(int x);
#endif
