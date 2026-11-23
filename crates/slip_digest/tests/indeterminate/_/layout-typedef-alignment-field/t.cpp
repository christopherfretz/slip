#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a field whose type is a typedef carrying `aligned`: the layout reads the typedef (kiln_abi `layout/mod.rs:443`), the type digest is canonical.
typedef int I __attribute__((aligned(8)));
struct S { char c; I i; };
