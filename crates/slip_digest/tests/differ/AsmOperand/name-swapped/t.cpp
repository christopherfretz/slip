#include "side.h"
// §6 AsmOperand `name-swapped`: the names of two operands swapped: the presence bytes are the same.
#ifdef B
void f(int x, int y) { asm("%[a] %[b]" : [b] "=r"(x), [a] "=r"(y)); }
#else
void f(int x, int y) { asm("%[a] %[b]" : [a] "=r"(x), [b] "=r"(y)); }
#endif
