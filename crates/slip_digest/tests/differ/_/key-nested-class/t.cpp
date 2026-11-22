#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a nested class.
#ifdef B
struct A { struct C {}; };
#else
struct A { struct B {}; };
#endif
