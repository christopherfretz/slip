#include "side.h"
// WP-156 fix round 3 (L2, mutation 7): a type name alone, `X`, in a decl-specifier-seq two declarators share (a `REF_GROUP` entry), binds to `n::X` on side A and to `::X` on side B (the same size). The entry is each declaration's, and T3 (the folded width) writes it in both operand units.
struct X { int b; };
namespace n {
#ifndef B
struct X { int a; };
#endif
_BitInt(sizeof(X) * 8) bi = 0, bj = 0;
}
