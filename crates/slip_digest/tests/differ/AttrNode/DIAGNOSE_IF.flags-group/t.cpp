#include "side.h"
// §10.1 AttrNode: `DIAGNOSE_IF.flags-group`: the fact differs.
#ifdef B
void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning", "format")));
#else
void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));
#endif
