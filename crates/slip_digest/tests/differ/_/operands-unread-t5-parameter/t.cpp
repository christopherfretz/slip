#include "side.h"
// WP-156 fix round 3 (L4): a type name alone, `X`, in a slot Kiln does not keep, binds to `n::X` on side A and to `::X` on side B (the same size). T5, the friend's holder: Kiln gives a parameter written in a friend specialisation's declarator a `ParmVar` whose context is the class, linked to no function, so the befriending class's unit writes every entry; the trigger writes the name (site 9): the operand unit changes.
struct X { int b; };
namespace n {
#ifndef B
struct X { int a; };
#endif
template <class T> void ftn(T) {}
struct FN { friend void ftn<>(int (*)[sizeof(X)]); };
}
