#include "side.h"
// §4.5 CallConv `PreserveMost`.
#ifdef B
void (*v)();
#else
void (__attribute__((preserve_most)) *v)();
#endif
