#include "side.h"
// §9.5 TemplateTables `trailing_requires`: a function's trailing
// requires-clause (its presence; it is also in the key).
#ifdef B
template <class T> struct A { void f() requires true {} };
#else
template <class T> struct A { void f() {} };
#endif
