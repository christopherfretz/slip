#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a operator.
#ifdef B
struct C {}; bool operator!=(C, C);
#else
struct C {}; bool operator==(C, C);
#endif
