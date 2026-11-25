#include "side.h"
// spec/digest.md §1 rule 2: decltype non dependent does not matter.
#ifdef B
int x; decltype(x) v;
#else
int x; int v;
#endif
