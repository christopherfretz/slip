#include "side.h"
// §9.2 TemplateParamList `requires`: a requires-clause after the head (its
// presence; the expression is the expression area's).
#ifdef B
template <class T> requires true struct X {};
#else
template <class T> struct X {};
#endif
