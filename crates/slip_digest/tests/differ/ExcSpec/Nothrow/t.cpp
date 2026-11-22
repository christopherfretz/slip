#include "side.h"
// §4.5 ExcSpec `Nothrow`.
#ifdef B
void (*v)();
#else
void (*v)() noexcept;
#endif
