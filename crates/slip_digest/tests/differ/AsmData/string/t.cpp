#include "side.h"
// §6 AsmData `string`.
#ifdef B
void f() { asm("nop" : : ); }
#else
void f() { asm("" : : ); }
#endif
