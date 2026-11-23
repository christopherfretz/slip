#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the attribute on an alias-declaration.
#ifdef B
using T16 [[gnu::aligned(16)]] = int;
#else
using T16 = int;
#endif
struct S { char c; T16 i; };
static_assert(sizeof(S) == (alignof(S) == 16 ? 32 : 8));
