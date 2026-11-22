#include "side.h"
// §4.5 FuncProto `params`.
#ifdef B
void (*v)(char);
#else
void (*v)(int);
#endif
