#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `#pragma pack` around a class changes its layout; the class holds it (`ColdDeclData` `record_pack`).
#ifdef B
#pragma pack(push, 1)
struct S { char c; int i; };
#pragma pack(pop)

#else
struct S { char c; int i; };
#endif
