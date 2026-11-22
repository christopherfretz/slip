#include "side.h"
// §7.2 CastKind `AddressSpaceConversion`.
__attribute__((address_space(1))) int *p1; int *p;
#ifdef B
int *v = p;
#else
int *v = (int*)p1;
#endif
