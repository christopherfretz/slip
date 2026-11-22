#include "side.h"
// §4.5 CallConv `VectorCall`.
#ifdef B
void (*v)();
#else
void (__attribute__((vectorcall)) *v)();
#endif
