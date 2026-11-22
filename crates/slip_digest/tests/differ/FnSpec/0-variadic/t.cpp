#include "side.h"
// §5.4 FnSpec `0-variadic`: it is in the key too (§3).
#ifdef B
void f(int, ...);
#else
void f(int);
#endif
