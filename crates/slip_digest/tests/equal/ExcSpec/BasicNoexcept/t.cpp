#include "side.h"
// §4.5 ExcSpec `BasicNoexcept`: written as Nothrow.
#ifdef B
void (*v)() noexcept(true);
#else
void (*v)() noexcept;
#endif
