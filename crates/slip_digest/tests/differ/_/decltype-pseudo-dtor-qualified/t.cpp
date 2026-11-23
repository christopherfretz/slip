#include "side.h"
// §4.4, F5: `decltype(p->T::~T())` against `decltype(p->~T())`: the destroyed type is part of the profile.

#ifdef B
template<class T, class U> auto f(T* p) -> decltype(p->~T());
#else
template<class T, class U> auto f(T* p) -> decltype(p->T::~T());
#endif
