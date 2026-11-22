#include "side.h"
// §10.1 AttrKind: `DIAGNOSE_IF`: the attribute on one side only.
#ifdef B
void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));
#else
void f(int x);
#endif
