#include "side.h"
// §9.1 TemplLink `Specialization.0-index`: the specialisation's `SpecInfo` is in
// its unit (here its `kind`).
template <class T> struct X {};
#ifdef B
template struct X<int>;
#else
extern template struct X<int>;
#endif
