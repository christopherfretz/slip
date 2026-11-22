#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a function params.
#ifdef B
void f(long);
#else
void f(int);
#endif
