#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the attribute before `typedef` (no `alignas` form applies to a typedef).
#ifdef B
[[gnu::aligned(16)]] typedef int T16;
#else
typedef int T16;
#endif
struct S { char c; T16 i; };
static_assert(sizeof(S) == (alignof(S) == 16 ? 32 : 8));
