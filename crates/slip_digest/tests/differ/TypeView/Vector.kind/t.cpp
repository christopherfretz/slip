#include "side.h"
// §4.3 `Vector.kind`.
typedef int v4 __attribute__((vector_size(16)));
typedef long l2 __attribute__((vector_size(16)));
typedef int v2 __attribute__((vector_size(8)));
typedef int e4 __attribute__((ext_vector_type(4)));
#ifdef B
e4 v;
#else
v4 v;
#endif
