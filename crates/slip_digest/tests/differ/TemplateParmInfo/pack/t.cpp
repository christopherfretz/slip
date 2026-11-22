#include "side.h"
// §9.2 TemplateParmInfo `pack`: a template parameter pack.
#ifdef B
template <class... T> struct X;
#else
template <class T> struct X;
#endif
