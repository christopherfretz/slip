#include "side.h"
// §7 SourceLocValues: `column`: only the position differs.
#ifdef B
unsigned v =   __builtin_COLUMN();
#else
unsigned v = __builtin_COLUMN();
#endif
