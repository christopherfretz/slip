#include "side.h"
// §9.2 TemplateParmInfo `index`: the position of a parameter in its list.
#ifdef B
template <int N, class T> struct X;
#else
template <class T> struct X;
#endif
