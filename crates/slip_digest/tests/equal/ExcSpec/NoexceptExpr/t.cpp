#include "side.h"
// §4.5 ExcSpec `NoexceptExpr`: written as Nothrow.
#ifdef B
void (*v)() noexcept;
#else
void (*v)() noexcept(true);
#endif
