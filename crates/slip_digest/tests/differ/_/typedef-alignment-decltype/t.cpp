#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `decltype` of a variable of the typedef.
typedef int I __attribute__((aligned(8)));
#ifdef B
I v;
#else
int v;
#endif
struct S { char c; decltype(v) d; };
static_assert(sizeof(S) == (alignof(S) == 8 ? 16 : 8));
