#include "side.h"
// §4.5 FuncInfo `0-noreturn`.
#ifdef B
void (*v)();
#else
void (*v)() __attribute__((noreturn));
#endif
