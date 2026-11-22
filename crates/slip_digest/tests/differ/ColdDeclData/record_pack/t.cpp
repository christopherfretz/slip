#include "side.h"
// §5.7 ColdDeclData `record_pack`: the `#pragma pack` a class definition was parsed under.
#ifdef B
#pragma pack(1)
#endif
struct S { char c; int i; };
