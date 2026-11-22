#include "side.h"
// §10.1 AttrKind: `ANALYZER_NORETURN`: the attribute on one side only.
#ifdef B
__attribute__((analyzer_noreturn)) void f();
#else
void f();
#endif
