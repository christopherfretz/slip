#include "side.h"
// §9.5 TemplateTables `pseudo_dtor_first-type`: the first type-name of
// `p->S::F::~Q()`.
#ifdef B
template <class T> void f(T* p) { p->T::C::~Q(); }
#else
template <class T> void f(T* p) { p->T::A::~Q(); }
#endif
