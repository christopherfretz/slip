#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a class template.
#ifdef B
template <class T, class U> struct S {};
#else
template <class T> struct S {};
#endif
