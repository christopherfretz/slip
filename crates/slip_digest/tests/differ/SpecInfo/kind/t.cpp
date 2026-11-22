#include "side.h"
// §9.4 SpecInfo `kind`: an explicit instantiation declaration against a
// definition of the same specialisation.
template <class T> struct X {};
#ifdef B
template struct X<int>;
#else
extern template struct X<int>;
#endif
