#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a variable template.
#ifdef B
template <class T, class U> T v;
#else
template <class T> T v;
#endif
