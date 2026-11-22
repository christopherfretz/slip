#include "side.h"
// digest §3.4: partial specialisations differing by a pack are two keys.
template <class... T> struct X {};
#ifdef B
template <class T> struct X<T> {};
template <class... T> struct X<int, T...> {};
#else
template <class T> struct X<T> {};
#endif
