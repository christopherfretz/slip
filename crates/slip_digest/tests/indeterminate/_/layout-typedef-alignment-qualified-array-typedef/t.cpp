#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a cv-qualified array typedef (Kiln's `QualifiedArray` sugar, kiln_ast `types/canon.rs:100`) whose element is the typedef.
typedef long L __attribute__((aligned(4)));
typedef L LA[2];
struct S { char c; const LA a; };
