#include "side.h"
// §7.2 StringKind `Unevaluated`.
#ifdef B
static_assert(true);
#else
static_assert(true, "a");
#endif
