#include "side.h"
// §9.4 SpecInfo `template` (keyed): the template a specialisation specialises
// is its key's parent.
template <class T> struct X {};
template <class T> struct Y {};
#ifdef B
template <> struct Y<int> {};
#else
template <> struct X<int> {};
#endif
