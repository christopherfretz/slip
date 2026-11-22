#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 VectorKind `NeonPoly`: against `neon_vector_type`.
#ifdef B
typedef unsigned char T __attribute__((neon_vector_type(16)));
T v;
#else
typedef unsigned char T __attribute__((neon_polyvector_type(16)));
T v;
#endif
