#include "side.h"
// §4.3 Quals `0-restrict`.
#ifdef B
int* v;
#else
int* __restrict v;
#endif
