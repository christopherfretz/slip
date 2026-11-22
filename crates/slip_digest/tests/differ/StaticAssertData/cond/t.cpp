#include "side.h"
// §5.6 StaticAssertData `cond`: the condition.
#ifdef B
static_assert(sizeof(int) > 1);
#else
static_assert(sizeof(int) > 0);
#endif
