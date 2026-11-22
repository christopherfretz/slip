#include "side.h"
// §7 SourceLocValues: `file_name`: only the position differs.
#ifdef B
#line 10 "b.cpp"
const char *v = __builtin_FILE_NAME();
#else
#line 10 "a.cpp"
const char *v = __builtin_FILE_NAME();
#endif
