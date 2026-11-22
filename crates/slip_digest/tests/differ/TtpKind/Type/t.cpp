// std: c++26
#include "side.h"
// §9.2 TtpKind `Type`: a type-tt-parameter against a variable-tt-parameter.
#ifdef B
template <template <class> class P> struct X;
#else
template <template <class> auto P> struct X;
#endif
