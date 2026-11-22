#include "side.h"
// §7 PredefinedKind: `PrettyFunction`: the fact differs.
#ifdef B
const char *f(const char *s = __func__);
#else
const char *f(const char *s = __PRETTY_FUNCTION__);
#endif
