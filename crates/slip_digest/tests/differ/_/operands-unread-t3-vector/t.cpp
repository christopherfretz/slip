#include "side.h"
// WP-156 fix round 3 (L4, mutation 2): a type name alone, `X`, in a slot Kiln does not keep, binds to `n::X` on side A and to `::X` on side B (the same size). T3: a vector's count is folded; the trigger writes the name (site 9): the operand unit changes.
struct X { int b; };
namespace n {
#ifndef B
struct X { int a; };
#endif
typedef int v4 __attribute__((vector_size(sizeof(X) * 4)));
}
