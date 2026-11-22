#include "side.h"
// §9.2 TemplateParmInfo `depth`: the parameter of a member template of a class
// template is one level deeper than that of a template at namespace scope
// (same index, kind and count).
#ifdef B
template <class T> struct A { template <class U> static void g(U); };
#else
template <class T> struct A {};
template <class U> void g(U);
#endif
