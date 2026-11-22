#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a function template return.
#ifdef B
template <class T> T* f(int);
#else
template <class T> T f(int);
#endif
