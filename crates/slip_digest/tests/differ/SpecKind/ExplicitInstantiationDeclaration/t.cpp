#include "side.h"
// §9.4 SpecKind `ExplicitInstantiationDeclaration` against an explicit
// instantiation definition of a class with a field (same `from`; a
// function would differ by its instantiated definition too).
template <class T> struct Y { int m; };
#ifdef B
extern template struct Y<int>;
#else
template struct Y<int>;
#endif
