#include "side.h"
// spec/digest.md §1 rule 2: struct against class does not matter.
#ifdef B
class S; struct S { int x; };
#else
struct S; struct S { int x; };
#endif
