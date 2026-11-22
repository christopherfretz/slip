// std: c++26
#include "side.h"
// §9.2 TtpKind `Variable`: a variable-tt-parameter against a type-tt-parameter.
#ifdef B
template <template <class> auto P> struct X;
#else
template <template <class> class P> struct X;
#endif
