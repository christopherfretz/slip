#include "side.h"
// §7.2 CastKind `BooleanToSignedIntegral`. Kiln builds it only for a splat of a `bool` prvalue that is not an lvalue conversion.
typedef int V4 __attribute__((ext_vector_type(4)));
V4 q;
bool bb;
#ifdef B
V4 v = q + 1;
#else
V4 v = q + true;
#endif
