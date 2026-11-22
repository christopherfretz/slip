#include "side.h"
// §9.4 SpecInfo `from` (digest §8): an explicit instantiation instantiated from
// one partial specialisation against another with the same deduced
// arguments (a requires-clause in the head makes it another partial
// specialisation).
template <class T> struct X {};
#ifdef B
template <class T> requires true struct X<T*> {};
template struct X<int*>;
#else
template <class T> struct X<T*> {};
template struct X<int*>;
#endif
