#include "side.h"
// §4.4, F5: `decltype(p->~A())` against `decltype(p->~B2())`: the destroyed type is part of the profile.
struct A {}; struct B2 {};
#ifdef B
template<class T, class U> auto f(T* p) -> decltype(p->~B2());
#else
template<class T, class U> auto f(T* p) -> decltype(p->~A());
#endif
