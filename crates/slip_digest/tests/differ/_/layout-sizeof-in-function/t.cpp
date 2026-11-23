#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `sizeof` is not stored: the function holds `sizeof(S)` and the class decides it.
#ifdef B
#pragma pack(push, 1)
#endif
struct S { char c; int i; };
#ifdef B
#pragma pack(pop)
#endif
unsigned long f() { return sizeof(S); }
