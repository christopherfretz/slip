#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the field names a later plain redeclaration of a typedef whose first declaration carries `aligned` (Kiln's layout reads every redeclaration, kiln_abi `layout/props.rs:85`).
#ifdef B
typedef int I2 __attribute__((aligned(8)));
#else
typedef int I2;
#endif
typedef int I2;
struct S { char c; I2 i; };
static_assert(sizeof(S) == (alignof(S) == 8 ? 16 : 8));
