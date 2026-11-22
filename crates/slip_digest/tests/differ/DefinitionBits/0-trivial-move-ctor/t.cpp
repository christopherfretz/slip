#include "side.h"
// §5.5 DefinitionBits `0-trivial-move-ctor`.
#ifdef B
struct C { C(C&&) {} };
#else
struct C { C(C&&) = default; };
#endif
