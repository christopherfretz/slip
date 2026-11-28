#include "side.h"
// WP-156 fix round 3 (L4): a type name alone, `X`, in a slot Kiln does not keep, binds to `n::X` on side A and to `::X` on side B (the same size). T5: a friend specialisation's template arguments; the trigger writes the name (site 9): the operand unit changes.
struct X { int b; };
namespace n {
#ifndef B
struct X { int a; };
#endif
template <int N> void ftf() {}
struct F7 { friend void ftf<sizeof(X)>(); };
}
