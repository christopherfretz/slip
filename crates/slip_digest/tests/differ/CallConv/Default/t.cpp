#include "side.h"
// §4.5 CallConv `Default`.
#ifdef B
void (__attribute__((ms_abi)) *v)();
#else
void (*v)();
#endif
