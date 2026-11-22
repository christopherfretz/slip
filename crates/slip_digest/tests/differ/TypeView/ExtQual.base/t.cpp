#include "side.h"
// §4.3 `ExtQual.base`.
#ifdef B
__attribute__((address_space(1))) char* v;
#else
__attribute__((address_space(1))) int* v;
#endif
