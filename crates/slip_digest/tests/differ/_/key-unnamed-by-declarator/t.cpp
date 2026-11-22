#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a unnamed by declarator.
#ifdef B
struct { int a; } t;
#else
struct { int a; } s;
#endif
