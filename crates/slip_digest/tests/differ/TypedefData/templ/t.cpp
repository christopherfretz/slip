#include "side.h"
// §5.5 TypedefData `templ`: the MemberSpecInfo of a member typedef of an explicit instantiation.
#ifdef B
template <class T> struct S { typedef T t; }; template struct S<int>;
#else
template <class T> struct S { typedef T t; }; extern template struct S<int>;
#endif
