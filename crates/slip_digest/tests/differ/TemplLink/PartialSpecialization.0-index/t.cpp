#include "side.h"
// §9.1 TemplLink `PartialSpecialization.0-index`: the partial specialisation's
// `PartialSpecInfo` is in its unit: here a default in a template template
// parameter's own head, which the head in the key leaves out.
template <class T> struct X {};
#ifdef B
template <template <class = int> class P> struct X<P<char>> {};
#else
template <template <class> class P> struct X<P<char>> {};
#endif
