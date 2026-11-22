#include "side.h"
// §9.5 TemplateTables `expr_nns`: the qualifier of a dependent name.
#ifdef B
template <class T> int f() { return T::C::g(); }
#else
template <class T> int f() { return T::A::g(); }
#endif
