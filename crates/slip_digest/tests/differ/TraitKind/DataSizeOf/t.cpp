#include "side.h"
// §7 TraitKind: `DataSizeOf`: the fact differs.
#ifdef B
unsigned long v = sizeof(int);
#else
unsigned long v = __datasizeof(int);
#endif
