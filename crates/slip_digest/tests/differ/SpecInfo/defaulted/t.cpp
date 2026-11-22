#include "side.h"
// §9.4 SpecInfo `defaulted`: whether an argument equals its parameter's
// default.
#ifdef B
template <class T = long> struct X {};
template struct X<int>;
#else
template <class T = int> struct X {};
template struct X<int>;
#endif
