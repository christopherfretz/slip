#include "side.h"
// §5.5 DefinitionBits `0-user-declared-move-assign`.
#ifdef B
struct C { C& operator=(C&&); };
#else
struct C { };
#endif
