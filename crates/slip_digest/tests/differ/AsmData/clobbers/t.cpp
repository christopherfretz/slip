#include "side.h"
// §6 AsmData `clobbers`.
#ifdef B
void f() { asm("" : : : "memory"); }
#else
void f() { asm("" : : : ); }
#endif
