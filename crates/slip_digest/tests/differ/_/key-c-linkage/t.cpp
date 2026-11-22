#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a c linkage.
#ifdef B
extern "C" void f(long);
#else
extern "C" void f(int);
#endif
