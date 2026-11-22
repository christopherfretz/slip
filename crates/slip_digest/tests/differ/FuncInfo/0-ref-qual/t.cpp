#include "side.h"
// §4.5 FuncInfo `0-ref-qual`.
struct C {};
#ifdef B
void (C::*v)() &&;
#else
void (C::*v)() &;
#endif
