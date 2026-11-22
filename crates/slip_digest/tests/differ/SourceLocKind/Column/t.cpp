#include "side.h"
// §7 SourceLocKind: `Column`: the fact differs.
#ifdef B
unsigned v = __builtin_LINE();
#else
unsigned v = __builtin_COLUMN();
#endif
