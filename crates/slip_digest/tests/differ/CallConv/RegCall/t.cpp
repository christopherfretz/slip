#include "side.h"
// §4.5 CallConv `RegCall`.
#ifdef B
void (*v)();
#else
void (__attribute__((regcall)) *v)();
#endif
