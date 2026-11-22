#include "side.h"
// target: aarch64-unknown-linux-gnu
// args: -march=armv8-a+sve -msve-vector-bits=512
// §4.3 VectorKind `SveFixedData`: against a GCC vector of the same size.
#ifdef B
typedef int T __attribute__((vector_size(64)));
T v;
#else
typedef __SVInt32_t T __attribute__((arm_sve_vector_bits(512)));
T v;
#endif
