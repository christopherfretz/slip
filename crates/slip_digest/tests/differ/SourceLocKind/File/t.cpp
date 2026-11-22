#include "side.h"
// §7 SourceLocKind: `File`: the fact differs.
#ifdef B
const char * v = __builtin_FILE_NAME();
#else
const char * v = __builtin_FILE();
#endif
