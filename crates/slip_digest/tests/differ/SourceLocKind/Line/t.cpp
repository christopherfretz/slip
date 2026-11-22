#include "side.h"
// §7 SourceLocKind: `Line`: the fact differs.
#ifdef B
unsigned v = __builtin_COLUMN();
#else
unsigned v = __builtin_LINE();
#endif
