#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignof` of an array of the typedef.
typedef long L __attribute__((aligned(4)));
#ifdef B
unsigned long f() { return alignof(L[4]); }
#else
unsigned long f() { return alignof(long[4]); }
#endif
