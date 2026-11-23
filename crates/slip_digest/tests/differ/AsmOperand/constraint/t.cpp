#include "side.h"
// §6 AsmOperand `constraint`.
#ifdef B
void f(int x) { asm("" : "=&r"(x)); }
#else
void f(int x) { asm("" : "=r"(x)); }
#endif
