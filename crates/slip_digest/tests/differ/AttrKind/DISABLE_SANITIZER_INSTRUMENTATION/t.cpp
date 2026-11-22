#include "side.h"
// §10.1 AttrKind: `DISABLE_SANITIZER_INSTRUMENTATION`: the attribute on one side only.
#ifdef B
__attribute__((disable_sanitizer_instrumentation)) void f();
#else
void f();
#endif
