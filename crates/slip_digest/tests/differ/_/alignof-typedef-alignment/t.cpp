#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignof` of the typedef: the operand's type digest holds it.
typedef int I __attribute__((aligned(8)));
#ifdef B
unsigned long f() { return alignof(I); }
#else
unsigned long f() { return alignof(int); }
#endif
