#include "side.h"
// §4.5 NoexceptValue `False`: normalised to None.
#ifdef B
void (*v)();
#else
void (*v)() noexcept(false);
#endif
