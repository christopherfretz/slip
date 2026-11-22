#include "side.h"
// §10.1 AttrKind: `OMP_ASSUME`: the attribute on one side only.
#ifdef B
[[omp::assume("ompx_a")]] void f();
#else
void f();
#endif
