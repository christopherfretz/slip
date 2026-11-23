#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `#pragma pack` at a member class defined out of line.
struct O { struct S; };
#ifdef B
#pragma pack(push, 1)
struct O::S { char c; int i; };
#pragma pack(pop)

#else
struct O::S { char c; int i; };
#endif
