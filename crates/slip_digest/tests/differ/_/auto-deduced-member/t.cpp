#include "side.h"
// §4.7 TypeView `Auto.deduced`: a member `auto m()` of a class template, deduced as dependent, against `int m()`.
#ifdef B
template <class T> struct S { int m() { return T(); } };
#else
template <class T> struct S { auto m() { return T(); } };
#endif
