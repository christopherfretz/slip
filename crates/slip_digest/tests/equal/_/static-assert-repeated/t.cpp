#include "side.h"
// digest §3.4: two identical static_asserts in a scope are one unit.
static_assert(true);
#ifdef B
static_assert(true);
#endif
