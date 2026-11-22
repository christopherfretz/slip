#include "side.h"
// §5.5 DefinitionBits `0-trivial-default-ctor`.
#ifdef B
struct C { C() {} };
#else
struct C { C() = default; };
#endif
