#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a cv-qualified array typedef (Kiln's `QualifiedArray` sugar, kiln_ast `types/canon.rs:100`).
typedef long L __attribute__((aligned(4)));
#ifdef B
typedef L LA[2];
#else
typedef long LA[2];
#endif
struct S { char c; const LA a; };
static_assert(sizeof(S) == (alignof(S) == 4 ? 20 : 24));
