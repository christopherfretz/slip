#include "side.h"
// §5.5 DefinitionBits `0-user-declared-dtor`.
#ifdef B
struct C { ~C(); };
#else
struct C { };
#endif
