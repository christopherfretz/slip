#include "side.h"
// §7 PredefinedKind: `Func`: the fact differs.
#ifdef B
const char *f(const char *s = __FUNCTION__);
#else
const char *f(const char *s = __func__);
#endif
