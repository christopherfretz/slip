#include "side.h"
// §10.1 AttrNode: `DIAGNOSE_IF.a`: the fact differs.
#ifdef B
void f(int x) __attribute__((diagnose_if(x < 1, "m", "warning")));
#else
void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));
#endif
