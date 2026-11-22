#include "side.h"
// §4.5 CallConv `Win64`.
#ifdef B
void (*v)();
#else
void (__attribute__((ms_abi)) *v)();
#endif
