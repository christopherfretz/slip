#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a deduction guide.
#ifdef B
template <class T> struct S { S(T); }; S(char) -> S<long>;
#else
template <class T> struct S { S(T); }; S(int) -> S<long>;
#endif
