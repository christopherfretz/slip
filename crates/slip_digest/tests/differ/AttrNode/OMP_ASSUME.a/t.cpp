#include "side.h"
// §10.1 AttrNode: `OMP_ASSUME.a`: the fact differs.
#ifdef B
[[omp::assume("ompx_b")]] void f();
#else
[[omp::assume("ompx_a")]] void f();
#endif
