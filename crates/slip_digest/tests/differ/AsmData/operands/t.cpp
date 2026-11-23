#include "side.h"
// §6 AsmData `operands`.
#ifdef B
void f(int x) { asm("" : : "r"(x), "r"(x)); }
#else
void f(int x) { asm("" : : "r"(x)); }
#endif
