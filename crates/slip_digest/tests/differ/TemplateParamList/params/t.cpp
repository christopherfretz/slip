#include "side.h"
// §9.2 TemplateParamList `params`: the parameters, in order.
#ifdef B
template <int N, class T> struct X {};
#else
template <class T, int N> struct X {};
#endif
