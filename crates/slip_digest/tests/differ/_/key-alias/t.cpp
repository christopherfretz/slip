#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a alias.
#ifdef B
using Y = int;
#else
using X = int;
#endif
