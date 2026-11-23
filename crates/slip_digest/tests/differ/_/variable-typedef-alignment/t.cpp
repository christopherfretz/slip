#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a variable of a typedef carrying `aligned` has the typedef's alignment (kiln_abi `layout/mod.rs:518`, `decl_align`); its type digest is canonical (wrong equal).
typedef int I __attribute__((aligned(8)));
#ifdef B
I v;
#else
int v;
#endif
