// std: c++26
#include "side.h"
// §9.2 TtpKind `Concept`: a concept-tt-parameter against a variable-tt-parameter.
#ifdef B
template <template <class> concept P> struct X;
#else
template <template <class> auto P> struct X;
#endif
