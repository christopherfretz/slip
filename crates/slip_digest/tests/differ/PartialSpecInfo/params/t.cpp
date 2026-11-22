#include "side.h"
// §9.4 PartialSpecInfo `params`: the partial specialisation's head.
template <class... T> struct X {};
#ifdef B
template <class... T> struct X<T*...> {};
#else
template <class T> struct X<T*> {};
#endif
