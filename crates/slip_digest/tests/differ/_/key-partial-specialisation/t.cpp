#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a partial specialisation.
#ifdef B
template <class T> struct S {}; template <class T> struct S<T&> {};
#else
template <class T> struct S {}; template <class T> struct S<T*> {};
#endif
