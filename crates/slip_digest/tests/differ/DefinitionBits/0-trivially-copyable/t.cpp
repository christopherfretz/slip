#include "side.h"
// §5.5 DefinitionBits `0-trivially-copyable`.
#ifdef B
struct C { C(const C&); };
#else
struct C { C(const C&) = default; };
#endif
