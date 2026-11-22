#include "side.h"
// digest §6 _ `expr-qualifier`.
namespace ns { int x; }
namespace ns {
#ifdef B
int v1 = ::ns::x, v2 = x, v3 = x; }
#else
int v1 = x, v2 = ns::x, v3 = ::ns::x; }
#endif
