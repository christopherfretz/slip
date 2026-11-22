#include "side.h"
// §9.2 TemplateParmInfo `depth`: the parameters of a member template of a class
// template are one level deeper.
#ifdef B
template <class T> struct A { template <class U> struct M; };
struct C { template <class T, class U> struct M; };
#else
template <class T> struct A { template <class U> struct M; };
struct C { template <class U> struct M; };
#endif
