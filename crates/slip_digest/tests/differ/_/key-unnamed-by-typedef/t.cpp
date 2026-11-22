#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a unnamed by typedef.
#ifdef B
typedef struct { int a; } T;
#else
typedef struct { int a; } S;
#endif
