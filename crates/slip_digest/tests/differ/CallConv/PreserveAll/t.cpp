#include "side.h"
// §4.5 CallConv `PreserveAll`.
#ifdef B
void (*v)();
#else
void (__attribute__((preserve_all)) *v)();
#endif
