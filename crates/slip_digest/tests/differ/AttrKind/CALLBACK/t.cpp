#include "side.h"
// §10.1 AttrKind: `CALLBACK`: the attribute on one side only.
#ifdef B
void f(void (*cb)(int), int x) __attribute__((callback(cb, x)));
#else
void f(void (*cb)(int), int x);
#endif
