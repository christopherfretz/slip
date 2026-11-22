#include "side.h"
// §5.6 StaticAssertData `message`: a message against none.
#ifdef B
static_assert(true, "m");
#else
static_assert(true);
#endif
