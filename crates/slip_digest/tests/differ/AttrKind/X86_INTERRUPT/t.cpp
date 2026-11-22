#include "side.h"
// §10.1 AttrKind: `X86_INTERRUPT`: the attribute on one side only.
#ifdef B
struct F; __attribute__((interrupt)) void f(F *);
#else
struct F; void f(F *);
#endif
