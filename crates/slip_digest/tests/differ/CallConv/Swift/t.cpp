#include "side.h"
// §4.5 CallConv `Swift`.
#ifdef B
void (*v)();
#else
void (__attribute__((swiftcall)) *v)();
#endif
