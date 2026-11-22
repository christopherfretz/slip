#include "side.h"
// spec/digest-kinds.md §4.3 BuiltinType `Int128`.
#ifdef B
int v;
#else
__int128 v;
#endif
