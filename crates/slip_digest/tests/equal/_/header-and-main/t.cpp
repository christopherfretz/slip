#include "side.h"
// spec/digest.md §1 rule 2: header and main does not matter.
#ifdef B
#include "h.h"
void f(int) {}
#else
#include "h.h"
void f(int) {}
#endif
