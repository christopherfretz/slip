// std: c++26
#include "side.h"
// §9.2 TemplateParmInfo `ttp_kind`: a type-tt-parameter against a
// concept-tt-parameter.
#ifdef B
template <template <class> concept P> struct X;
#else
template <template <class> class P> struct X;
#endif
