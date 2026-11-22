#include "side.h"
// §10.1 rule: the encoding `attrs`: the `inherited` copy of an asm label adds nothing.
#ifdef B
void f() asm("g");
void f();
#else
void f() asm("g");
#endif
