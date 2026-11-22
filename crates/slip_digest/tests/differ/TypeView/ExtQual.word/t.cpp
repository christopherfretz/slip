#include "side.h"
// §4.3 `ExtQual.word`.
#ifdef B
__attribute__((address_space(2))) int* v;
#else
__attribute__((address_space(1))) int* v;
#endif
