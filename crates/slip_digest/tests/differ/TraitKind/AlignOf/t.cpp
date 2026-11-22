#include "side.h"
// §7 TraitKind: `AlignOf`: the fact differs.
#ifdef B
unsigned long v = __alignof(int);
#else
unsigned long v = alignof(int);
#endif
