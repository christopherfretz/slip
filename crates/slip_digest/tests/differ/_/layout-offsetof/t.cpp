#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `offsetof` stores its result (`OffsetOfExpr.b`).
#ifdef B
#pragma pack(push, 1)
#endif
struct S { char c; int i; };
#ifdef B
#pragma pack(pop)
#endif
unsigned long f() { return __builtin_offsetof(S, i); }
