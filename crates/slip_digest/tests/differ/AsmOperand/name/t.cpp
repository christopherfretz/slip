#include "side.h"
// §6 AsmOperand `name`.
#ifdef B
void f(int x) { asm("" : : [v] "r"(x)); }
#else
void f(int x) { asm("" : : "r"(x)); }
#endif
