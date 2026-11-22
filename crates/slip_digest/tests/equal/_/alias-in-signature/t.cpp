#include "side.h"
// spec/digest.md §1 rule 2: alias in signature does not matter.
#ifdef B
struct S {}; using A = S; void f(A);
#else
struct S {}; using A = S; void f(S);
#endif
