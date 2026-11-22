#include "side.h"
// §4.5 ExcSpec `NothrowAttr`: written as Nothrow.
#ifdef B
void (*v)() noexcept;
#else
void (*v)() __attribute__((nothrow));
#endif
