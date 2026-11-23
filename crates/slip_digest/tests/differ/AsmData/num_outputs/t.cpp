#include "side.h"
// §6 AsmData `num_outputs`: the second operand an output, not an input (its constraint differs too).
#ifdef B
void f(int x) { asm("" : "+r"(x), "+r"(x) : ); }
#else
void f(int x) { asm("" : "+r"(x) : "r"(x)); }
#endif
