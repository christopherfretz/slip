#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignof` of a reference to the typedef is the typedef's ([expr.alignof]/3; clang 19: 4; Kiln: 8, "For Kiln").
typedef long L __attribute__((aligned(4)));
#ifdef B
unsigned long f() { return alignof(L &); }
#else
unsigned long f() { return alignof(long &); }
#endif
