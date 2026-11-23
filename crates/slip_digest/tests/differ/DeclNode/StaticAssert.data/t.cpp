#include "side.h"
// §5.3 DeclNode `StaticAssert.data`: the message (content-keyed: the key moves).
#ifdef B
static_assert(true, "b");
#else
static_assert(true, "a");
#endif
