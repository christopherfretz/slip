#include "side.h"
// §4.5 ExcSpec `None`.
#ifdef B
void (*v)() noexcept;
#else
void (*v)();
#endif
