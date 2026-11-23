#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `new` of the typedef: the allocated type holds it.
typedef int I __attribute__((aligned(8)));
#ifdef B
I *f() { return new I; }
#else
int *f() { return new int; }
#endif
