#include "side.h"
// §5.5 DefinitionBits `0-user-declared-ctor`.
#ifdef B
struct C { C(int); };
#else
struct C { };
#endif
