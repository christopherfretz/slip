// std: c++26
#include "side.h"
// §9.5 TemplateTables `ttp_owner`: a concept template parameter names its
// template as its mangling parent (and the parameter's kind changes with it).
#ifdef B
template <template <class> concept P> struct X;
#else
template <template <class> auto P> struct X;
#endif
