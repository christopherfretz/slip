#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a field.
#ifdef B
struct S { int b; };
#else
struct S { int a; };
#endif
