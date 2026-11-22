#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a function method const.
#ifdef B
struct C { void m() const; };
#else
struct C { void m(); };
#endif
