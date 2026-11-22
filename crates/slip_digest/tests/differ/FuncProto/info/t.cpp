#include "side.h"
// §4.5 FuncProto `info`.
#ifdef B
void (*v)(int, ...);
#else
void (*v)(int);
#endif
