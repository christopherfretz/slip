#include "side.h"
// §10.1 AttrNode: `DIAGNOSE_IF.flags-arg-dependent`: the fact differs.
#ifdef B
void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));
#else
void f(int x) __attribute__((diagnose_if(true, "m", "warning")));
#endif
