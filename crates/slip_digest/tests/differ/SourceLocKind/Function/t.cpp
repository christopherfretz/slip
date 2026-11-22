#include "side.h"
// §7 SourceLocKind: `Function`: the fact differs.
#ifdef B
const char * v = __builtin_FILE();
#else
const char * v = __builtin_FUNCTION();
#endif
