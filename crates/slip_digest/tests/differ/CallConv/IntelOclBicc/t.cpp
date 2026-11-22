#include "side.h"
// §4.5 CallConv `IntelOclBicc`.
#ifdef B
void (*v)();
#else
void (__attribute__((intel_ocl_bicc)) *v)();
#endif
