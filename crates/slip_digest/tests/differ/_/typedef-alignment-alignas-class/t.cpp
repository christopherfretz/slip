#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignas(type-id)` naming the typedef: the attribute's type holds it.
typedef int I __attribute__((aligned(8)));
#ifdef B
struct alignas(I) S { char c; };
#else
struct alignas(int) S { char c; };
#endif
static_assert(alignof(S) == (sizeof(S) == 8 ? 8 : 4));
