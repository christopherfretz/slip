#include "side.h"
// §7.4 ColdExprData `cast_written-cast`.
int a;
#ifdef B
int v = static_cast<int>(a);
#else
int v = static_cast<const int>(a);
#endif
