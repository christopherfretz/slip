#include "side.h"
// §4.5 FuncInfo `0-this-const`.
struct C {};
#ifdef B
void (C::*v)();
#else
void (C::*v)() const;
#endif
