#include "side.h"
// §7 SourceLocValues: `ctx`: only the position differs.
#ifdef B


const char *f(const char *s = __builtin_FUNCTION());
#else
const char *f(const char *s = __builtin_FUNCTION());
#endif
