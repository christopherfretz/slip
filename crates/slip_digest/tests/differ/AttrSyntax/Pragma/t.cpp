#include "side.h"
// §10.1 AttrSyntax: `Pragma`: `#pragma weak` against `__attribute__((weak))`.
#ifdef B
void f();
#pragma weak f
#else
__attribute__((weak)) void f();
#endif
