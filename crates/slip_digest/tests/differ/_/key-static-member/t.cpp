#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a static member.
#ifdef B
struct C { static int w; };
#else
struct C { static int v; };
#endif
