#include "side.h"
// §7.1 ObjectKind `VectorComponent`.
typedef int V4 __attribute__((ext_vector_type(4)));
V4 q;
int arr[4];
#ifdef B
int v = arr[0];
#else
int v = q[0];
#endif
