#include "side.h"
// §5.5 DefinitionBits `0-user-provided-default-ctor`.
#ifdef B
struct C { C() {} int a; };
#else
struct C { C() = default; int a; };
#endif
