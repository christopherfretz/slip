#include "side.h"
// §4.5 ExcSpec `NoexceptExpr.expr`: written as Nothrow.
#ifdef B
void (*v)() noexcept(true);
#else
void (*v)() noexcept(1 + 1 == 2);
#endif
