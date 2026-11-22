#include "side.h"
// §7.2 CastKind `BooleanToSignedIntegral`: Kiln builds it for the splat of
// a `bool` to an ext vector (kiln_sema vector_ops.rs:246), here in a cast.
typedef int V4 __attribute__((ext_vector_type(4)));
bool bb;
#ifdef B
V4 v = (V4)1;
#else
V4 v = (V4)bb;
#endif
