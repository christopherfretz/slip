#include "side.h"
// §9.4 PartialSpecInfo `args`: the argument pattern as written; an alias in
// it is spelling.
template <class T> struct X {};
template <class T> using P = T*;
#ifdef B
template <class T> struct X<P<T>> {};
#else
template <class T> struct X<T*> {};
#endif
