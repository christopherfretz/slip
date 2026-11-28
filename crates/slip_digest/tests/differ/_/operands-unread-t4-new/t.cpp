#include "side.h"
// WP-156 fix round 3 (L4): a type name alone, `X`, in a slot Kiln does not keep, binds to `n::X` on side A and to `::X` on side B (the same size). T4 for `new`: a constrained placeholder in a `new`; the trigger writes the name (site 9): the operand unit changes.
struct X { int b; };
namespace n {
#ifndef B
struct X { int a; };
#endif
template <class T, class U> concept C = sizeof(T) == sizeof(U);
int *p = new C<X> auto(1);
}
