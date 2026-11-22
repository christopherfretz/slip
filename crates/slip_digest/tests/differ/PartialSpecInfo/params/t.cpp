#include "side.h"
// §9.4 PartialSpecInfo `params`: the partial specialisation's head as a unit
// writes it: a default in a template template parameter's own head, which
// the head in the key leaves out.
template <class T> struct X {};
#ifdef B
template <template <class = int> class P> struct X<P<char>> {};
#else
template <template <class> class P> struct X<P<char>> {};
#endif
