#include "side.h"
// §4.5 RefQual `None`.
struct C {};
#ifdef B
void (C::*v)() &;
#else
void (C::*v)();
#endif
