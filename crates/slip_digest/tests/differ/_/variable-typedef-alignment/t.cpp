#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a variable of the typedef (kiln_abi `layout/mod.rs:518`, `decl_align`).
typedef int I __attribute__((aligned(8)));
#ifdef B
I v;
#else
int v;
#endif
