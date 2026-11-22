#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a unnamed ordinal.
#ifdef B
struct S { struct { int a; }; struct { int b; }; };
#else
struct S { struct { int a; }; };
#endif
