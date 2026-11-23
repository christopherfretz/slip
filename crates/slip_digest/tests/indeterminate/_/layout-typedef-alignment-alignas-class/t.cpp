#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `alignas(type-id)` naming the typedef: the alignment is the typedef's (kiln_abi `layout/props.rs:90`), the attribute's type digest canonical.
typedef int I __attribute__((aligned(8)));
struct alignas(I) S { char c; };
