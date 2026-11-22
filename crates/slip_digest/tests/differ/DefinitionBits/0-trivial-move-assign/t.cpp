#include "side.h"
// §5.5 DefinitionBits `0-trivial-move-assign`.
#ifdef B
struct C { C& operator=(C&&) { return *this; } };
#else
struct C { C& operator=(C&&) = default; };
#endif
