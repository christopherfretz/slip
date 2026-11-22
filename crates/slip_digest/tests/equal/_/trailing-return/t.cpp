#include "side.h"
// spec/digest.md §1 rule 2: trailing return does not matter.
#ifdef B
auto f() -> int;
#else
int f();
#endif
