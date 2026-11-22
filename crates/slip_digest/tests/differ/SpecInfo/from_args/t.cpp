#include "side.h"
// §9.4 SpecInfo `from_args`: the arguments deduced for the partial
// specialisation chosen (`from` changes with them: no edit changes one alone).
template <class T> struct X {};
#ifdef B
template <class T> struct X<T**> {};
template struct X<int**>;
#else
template <class T> struct X<T*> {};
template struct X<int**>;
#endif
