// std: c++17
#include "side.h"
// §4.5 ExcSpec `DynamicNone`: `throw()` is written as Nothrow.
#ifdef B
void (*v)() noexcept;
#else
void (*v)() throw();
#endif
