#include "side.h"
// §10.1 AttrNode: `MIN_VECTOR_WIDTH.a`: the fact differs.
#ifdef B
__attribute__((min_vector_width(256))) void f();
#else
__attribute__((min_vector_width(128))) void f();
#endif
