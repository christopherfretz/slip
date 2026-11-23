#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignof(decltype(f()))` (clang 19: 8 / 4): the return type is not walked; the call's type is written in the user.
typedef int I __attribute__((aligned(8)));
#ifdef B
I f();
#else
int f();
#endif
unsigned long g() { return alignof(decltype(f())); }
