#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an array of a typedef that lowers alignment (an over-aligned element is an error).
typedef long L __attribute__((aligned(4)));
#ifdef B
struct S { char c; L a[2]; };
#else
struct S { char c; long a[2]; };
#endif
static_assert(sizeof(S) == (alignof(S) == 4 ? 20 : 24));
