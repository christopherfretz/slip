#include "side.h"
// §7 SourceLocValues: `line`: only the position differs.
#ifdef B
#line 20
unsigned v = __builtin_LINE();
#else
#line 10
unsigned v = __builtin_LINE();
#endif
