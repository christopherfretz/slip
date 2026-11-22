#include "side.h"
// §4.5 FuncInfo `0-this-restrict`.
struct C {};
#ifdef B
void (C::*v)();
#else
void (C::*v)() __restrict;
#endif
