#include "side.h"
// §5.5 DefinitionBits `0-trivial-dtor`.
#ifdef B
struct C { ~C() {} };
#else
struct C { ~C() = default; };
#endif
