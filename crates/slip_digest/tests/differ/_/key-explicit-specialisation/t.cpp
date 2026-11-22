#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a explicit specialisation.
#ifdef B
template <class T> struct S {}; template <> struct S<long> {};
#else
template <class T> struct S {}; template <> struct S<int> {};
#endif
