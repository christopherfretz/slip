#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a function template.
#ifdef B
template <class T> void f(T*);
#else
template <class T> void f(T);
#endif
