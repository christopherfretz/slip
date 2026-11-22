#include "side.h"
// §10.1 AttrKind: `MIN_VECTOR_WIDTH`: the attribute on one side only.
#ifdef B
__attribute__((min_vector_width(128))) void f();
#else
void f();
#endif
