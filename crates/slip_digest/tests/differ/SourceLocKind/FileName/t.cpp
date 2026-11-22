#include "side.h"
// §7 SourceLocKind: `FileName`: the fact differs.
#ifdef B
const char * v = __builtin_FUNCTION();
#else
const char * v = __builtin_FILE_NAME();
#endif
