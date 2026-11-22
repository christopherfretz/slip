#include "side.h"
// §7 PredefinedKind: `Function`: the fact differs.
#ifdef B
const char *f(const char *s = __func__);
#else
const char *f(const char *s = __FUNCTION__);
#endif
