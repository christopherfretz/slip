#include "side.h"
// §4.5 CallConv `SwiftAsync`.
#ifdef B
void (*v)();
#else
void (__attribute__((swiftasynccall)) *v)();
#endif
