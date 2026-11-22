#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a function variadic.
#ifdef B
void f(int, ...);
#else
void f(int);
#endif
