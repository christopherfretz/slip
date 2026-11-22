#include "side.h"
// §5.5 DefinitionBits `0-anonymous`.
#ifdef B
struct S { struct { int a; }; };
#else
struct S { struct { int a; } x; };
#endif
