#include "side.h"
// §6 AsmData `labels`.
#ifdef B
void f() { asm goto("" : : : : m); l: ; m: ; }
#else
void f() { asm goto("" : : : : l); l: ; m: ; }
#endif
