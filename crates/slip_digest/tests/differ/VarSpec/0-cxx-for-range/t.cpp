#include "side.h"
// §5.4 VarSpec `0-cxx-for-range`: the variable of a range-based for.
#ifdef B
void f(int (&a)[2]) { for (int e : a) {} }
#else
void f(int (&a)[2]) { for (int e = 0; e < 2; ++e) {} }
#endif
