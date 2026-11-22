#include "side.h"
// §5.5 DefinitionBits `0-user-declared-copy-ctor`.
#ifdef B
struct C { C(const C&); };
#else
struct C { };
#endif
