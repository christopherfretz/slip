#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a function ref qual.
#ifdef B
struct C { void m() &&; };
#else
struct C { void m() &; };
#endif
