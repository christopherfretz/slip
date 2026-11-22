// std: c++26
#include "side.h"
// §6 ExpansionData `exprs`: in a pattern: once expanded, each expansion holds its expression too.
#ifdef B
template <class T> void f(T) { template for (int e : {1, 3}) ; }
#else
template <class T> void f(T) { template for (int e : {1, 2}) ; }
#endif
