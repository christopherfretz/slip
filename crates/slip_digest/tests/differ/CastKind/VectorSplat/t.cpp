#include "side.h"
// §7.2 CastKind `VectorSplat`.
typedef int V4 __attribute__((ext_vector_type(4)));
V4 q;
#ifdef B
V4 v = q + q;
#else
V4 v = q + 1;
#endif
