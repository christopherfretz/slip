#include "side.h"
// spec/digest.md §1 rule 2: elaborated does not matter.
#ifdef B
struct S {}; struct S v;
#else
struct S {}; S v;
#endif
