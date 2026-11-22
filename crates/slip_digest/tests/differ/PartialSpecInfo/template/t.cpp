#include "side.h"
// §9.4 PartialSpecInfo `template` (keyed): the primary template is the key's
// parent.
template <class T> struct X {};
template <class T> struct Y {};
#ifdef B
template <class T> struct Y<T*> {};
#else
template <class T> struct X<T*> {};
#endif
