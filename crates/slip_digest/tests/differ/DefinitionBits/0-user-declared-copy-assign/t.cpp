#include "side.h"
// §5.5 DefinitionBits `0-user-declared-copy-assign`.
#ifdef B
struct C { C& operator=(const C&); };
#else
struct C { };
#endif
