#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a destructor.
#ifdef B
struct C { ~C(); };
#else
struct C { };
#endif
