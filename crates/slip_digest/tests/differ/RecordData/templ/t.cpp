#include "side.h"
// §5.5 RecordData `templ`: the SpecInfo of an explicit instantiation (declaration against definition).
#ifdef B
template <class T> struct X {}; template struct X<int>;
#else
template <class T> struct X {}; extern template struct X<int>;
#endif
