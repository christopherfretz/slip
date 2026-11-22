#include "side.h"
// §4.5 CallConv `PreserveNone`.
#ifdef B
void (*v)();
#else
void (__attribute__((preserve_none)) *v)();
#endif
