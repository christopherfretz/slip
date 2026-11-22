#include "side.h"
// §5.5 EnumData `templ`: the MemberSpecInfo of a member enumeration of an explicit instantiation.
#ifdef B
template <class T> struct X { enum E : int { a }; }; template struct X<int>;
#else
template <class T> struct X { enum E : int { a }; }; extern template struct X<int>;
#endif
