#include "side.h"
// spec/digest.md §1 rule 2: typedef in signature does not matter.
#ifdef B
struct S {}; typedef S* P; void f(P);
#else
struct S {}; typedef S* P; void f(S*);
#endif
