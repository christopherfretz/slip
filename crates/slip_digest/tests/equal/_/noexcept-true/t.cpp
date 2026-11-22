#include "side.h"
// §4.5 NoexceptValue `True`: normalised to Nothrow.
#ifdef B
void (*v)() noexcept;
#else
void (*v)() noexcept(true);
#endif
