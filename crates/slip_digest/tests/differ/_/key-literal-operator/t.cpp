#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a literal operator.
#ifdef B
int operator""_b(unsigned long long);
#else
int operator""_a(unsigned long long);
#endif
