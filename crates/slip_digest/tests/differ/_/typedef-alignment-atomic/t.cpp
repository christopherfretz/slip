#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an atomic of the typedef, too wide to be promoted: its alignment is the typedef's.
struct Big { char b[32]; };
typedef Big BA __attribute__((aligned(64)));
#ifdef B
struct S { char c; _Atomic(BA) a; };
#else
struct S { char c; _Atomic(Big) a; };
#endif
static_assert(sizeof(S) == (alignof(S) == 64 ? 128 : 33));
