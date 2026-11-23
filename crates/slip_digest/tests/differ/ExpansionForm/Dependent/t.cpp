// std: c++26
#include "side.h"
// §6 ExpansionForm `Dependent`.
#ifdef B
template <class T> void f(T t) { template for (int e : t) ; }
#else
template <class T> void f(T t) { template for (int e : {1, 2}) ; }
#endif
