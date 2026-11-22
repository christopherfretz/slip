// std: c++26
#include "side.h"
// §6 ExpansionData `range_init`.
static constexpr int a[] = {1, 2};
static constexpr int b[] = {1, 2};
#ifdef B
template <class T> void f(T) { template for (constexpr int e : b) ; }
#else
template <class T> void f(T) { template for (constexpr int e : a) ; }
#endif
