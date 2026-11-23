#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignof` of a typedef carrying `aligned` is the typedef's alignment; the operand's type digest is canonical, so `f` is equal while its value differs (wrong equal).
typedef int I __attribute__((aligned(8)));
#ifdef B
unsigned long f() { return alignof(I); }
#else
unsigned long f() { return alignof(int); }
#endif
