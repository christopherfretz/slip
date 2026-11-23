// std: c++26
#include "side.h"
// §6 ExpansionData `decl`: in a pattern: once expanded, each expansion holds the variable too.
#ifdef B
template <class T> void f(T) { template for (long e : {1, 2}) ; }
#else
template <class T> void f(T) { template for (int e : {1, 2}) ; }
#endif
