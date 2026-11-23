#include "side.h"
// §4.4, F5: `decltype(p->~T())` against `decltype(p->~U())`: the destroyed type is part of the profile.

#ifdef B
template<class T, class U> auto f(T* p) -> decltype(p->~U());
#else
template<class T, class U> auto f(T* p) -> decltype(p->~T());
#endif
