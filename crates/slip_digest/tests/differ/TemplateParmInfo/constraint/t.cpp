#include "side.h"
// §9.2 TemplateParmInfo `constraint`: a type parameter's type-constraint (its
// presence; the expression is the expression area's).
template <class T> concept C = true;
#ifdef B
template <C T> struct X;
#else
template <class T> struct X;
#endif
