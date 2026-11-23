#include "side.h"
// §6 AsmOperand `expr`.
#ifdef B
void f(int x, int y) { asm("" : : "r"(y)); }
#else
void f(int x, int y) { asm("" : : "r"(x)); }
#endif
