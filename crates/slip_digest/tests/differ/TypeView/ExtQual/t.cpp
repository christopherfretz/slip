#include "side.h"
// §4.3 TypeView `ExtQual`.
#ifdef B
int* v;
#else
__attribute__((address_space(1))) int* v;
#endif
