#include "side.h"
// spec/digest-kinds.md §4.3 BuiltinType `UInt128`.
#ifdef B
int v;
#else
unsigned __int128 v;
#endif
