#include "side.h"
// §10.1 AttrKind: `ASM_LABEL`: the attribute on one side only.
#ifdef B
void f() asm("g");
#else
void f();
#endif
