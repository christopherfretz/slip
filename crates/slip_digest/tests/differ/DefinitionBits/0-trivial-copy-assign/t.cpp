#include "side.h"
// §5.5 DefinitionBits `0-trivial-copy-assign`.
#ifdef B
struct C { C& operator=(const C&) { return *this; } };
#else
struct C { C& operator=(const C&) = default; };
#endif
