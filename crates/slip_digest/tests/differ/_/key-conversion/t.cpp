#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a conversion.
#ifdef B
struct C { operator long(); };
#else
struct C { operator int(); };
#endif
