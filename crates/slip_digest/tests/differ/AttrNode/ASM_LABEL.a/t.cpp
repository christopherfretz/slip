#include "side.h"
// §10.1 AttrNode: `ASM_LABEL.a`: the fact differs.
#ifdef B
void f() asm("h");
#else
void f() asm("g");
#endif
