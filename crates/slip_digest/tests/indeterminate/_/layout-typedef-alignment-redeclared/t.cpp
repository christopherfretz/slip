#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the field names a later redeclaration of a typedef whose first declaration carries `aligned` (Kiln reads every redeclaration, kiln_abi `layout/props.rs:85`).
typedef int I2 __attribute__((aligned(8)));
typedef int I2;
struct S { char c; I2 i; };
