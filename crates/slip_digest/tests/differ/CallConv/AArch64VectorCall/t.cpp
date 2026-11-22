#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.5 CallConv `AArch64VectorCall`: against `aarch64_sve_pcs`.
#ifdef B
void (__attribute__((aarch64_sve_pcs)) *v)();
#else
void (__attribute__((aarch64_vector_pcs)) *v)();
#endif
