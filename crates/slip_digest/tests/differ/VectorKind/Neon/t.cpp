#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 VectorKind `Neon`: against a GCC vector of the same size.
#ifdef B
typedef int T __attribute__((vector_size(16)));
T v;
#else
typedef int T __attribute__((neon_vector_type(4)));
T v;
#endif
