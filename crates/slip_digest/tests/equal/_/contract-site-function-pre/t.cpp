// std: c++26
#include "side.h"
// WP-034 (§6): `__PRETTY_FUNCTION__` names the template parameter, of a precondition (ContractSite).
#ifdef B
template <class U> void f(U) pre(true) {}
template void f<int>(int);
#else
template <class T> void f(T) pre(true) {}
template void f<int>(int);
#endif
