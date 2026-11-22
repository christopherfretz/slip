#include "side.h"
// §7 TraitKind: `PreferredAlignOf`: the fact differs.
#ifdef B
unsigned long v = sizeof(int);
#else
unsigned long v = __alignof(int);
#endif
