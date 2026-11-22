#include "side.h"
// §5.5 DefinitionBits `0-user-declared-move-ctor`.
#ifdef B
struct C { C(C&&); };
#else
struct C { };
#endif
