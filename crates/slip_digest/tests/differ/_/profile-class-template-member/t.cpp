#include "side.h"
// §4.4, §7 _ `profile-class-template-member`: a profile in a class template's member signature.
#ifdef B
template<class T> struct S { template<class U> static auto f(U u) -> decltype(u + T()); template<class U> static auto f(U u) -> decltype(u - T()); };
#else
template<class T> struct S { template<class U> static auto f(U u) -> decltype(u + T()); };
#endif
