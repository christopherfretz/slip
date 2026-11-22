#include "side.h"
// §9.4 SpecInfo `from` and `from_args`: an explicit instantiation of the
// primary template against one of a partial specialisation.
template <class T> struct X {};
#ifdef B
template <class T> struct X<T*> {};
template struct X<int*>;
#else
template struct X<int*>;
#endif
