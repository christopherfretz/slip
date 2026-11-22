#include "side.h"
// spec/digest.md §1 rule 2: parameter names does not matter.
#ifdef B
void f(int b);
#else
void f(int a);
#endif
