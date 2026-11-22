#include "side.h"
// §4.5 FuncInfo `0-variadic`.
#ifdef B
void (*v)(int, ...);
#else
void (*v)(int);
#endif
