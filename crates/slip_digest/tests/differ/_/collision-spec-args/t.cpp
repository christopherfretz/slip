#include "side.h"
// digest §3.4: two explicit specialisations differing in an argument are two
// keys (no collision).
template <class T> struct X {};
#ifdef B
template <> struct X<int> {};
template <> struct X<unsigned> {};
#else
template <> struct X<int> {};
#endif
