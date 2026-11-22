#include "side.h"
// §9.4 SpecInfo `args` (keyed): the canonical arguments are the key's
// discriminator.
template <class T> struct X {};
#ifdef B
template <> struct X<long> {};
#else
template <> struct X<int> {};
#endif
