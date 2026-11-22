#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a alias template.
#ifdef B
template <class T> using A2 = T*;
#else
template <class T> using A1 = T*;
#endif
