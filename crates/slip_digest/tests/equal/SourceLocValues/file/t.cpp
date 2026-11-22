#include "side.h"
// §7 SourceLocValues: `file`: only the position differs.
#ifdef B
#line 10 "b.cpp"
const char *v = __builtin_FILE();
#else
#line 10 "a.cpp"
const char *v = __builtin_FILE();
#endif
