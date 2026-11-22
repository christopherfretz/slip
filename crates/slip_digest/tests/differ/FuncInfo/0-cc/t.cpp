#include "side.h"
// §4.5 FuncInfo `0-cc`.
#ifdef B
void (*v)();
#else
void (__attribute__((ms_abi)) *v)();
#endif
