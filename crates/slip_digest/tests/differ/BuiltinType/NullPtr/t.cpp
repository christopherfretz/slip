#include "side.h"
// spec/digest-kinds.md §4.3 BuiltinType `NullPtr`.
#ifdef B
int v;
#else
decltype(nullptr) v;
#endif
