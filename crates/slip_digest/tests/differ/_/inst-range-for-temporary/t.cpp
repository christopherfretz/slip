#include "side.h"
// digest §4.3 "Numbering an owner" (WP-123): a range-for over a prvalue in a function template's body. Kiln shares the non-dependent temporary with the specialisation, whose LetData `extending` names the pattern's `__range` variable: `Ref::outer` to the pattern, which is numbered before the first unit. The range's type differs.
struct R { int *b, *e; int *begin() const { return b; } int *end() const { return e; } };
struct Q { long *b, *e; long *begin() const { return b; } long *end() const { return e; } };
R make_r();
Q make_q();
#ifdef B
template <class T> int f(T) { int s = 0; for (int x : make_q()) s += x; return s; }
#else
template <class T> int f(T) { int s = 0; for (int x : make_r()) s += x; return s; }
#endif
template int f<int>(int);
