#include "side.h"
// WP-156 fix round 3 (L4 T4, mutation 8): a control. A variable whose type a constrained placeholder deduces (T4), whose initialiser spells a type by a typedef on side A and directly on side B. T4's range ends at the declaration's last concept name, so the initialiser's type names are not written (its stream writes the type): equal. With the whole declaration as the range, side A's `I` would be written.
template <class T> concept C = true;
namespace n { using I = int; }
#ifdef B
C auto v = sizeof(int);
#else
C auto v = sizeof(n::I);
#endif
