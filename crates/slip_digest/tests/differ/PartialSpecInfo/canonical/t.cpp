#include "side.h"
// §9.4 PartialSpecInfo `canonical` (keyed): the argument pattern.
template <class T> struct X {};
#ifdef B
template <class T> struct X<T&> {};
#else
template <class T> struct X<T*> {};
#endif
