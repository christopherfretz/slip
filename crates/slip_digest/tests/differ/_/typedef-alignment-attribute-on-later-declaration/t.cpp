#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the attribute on a later redeclaration of the typedef.
typedef int R2;
#ifdef B
typedef int R2 __attribute__((aligned(8)));
#else
typedef int R2;
#endif
struct S { char c; R2 i; };
static_assert(sizeof(S) == (alignof(S) == 8 ? 16 : 8));
