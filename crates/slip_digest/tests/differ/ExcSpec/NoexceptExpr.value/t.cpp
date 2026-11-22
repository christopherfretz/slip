#include "side.h"
// §4.5 ExcSpec `NoexceptExpr.value`.
#ifdef B
void (*v)() noexcept(false);
#else
void (*v)() noexcept(true);
#endif
