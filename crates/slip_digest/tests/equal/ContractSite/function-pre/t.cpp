// std: c++26
#include "side.h"
// §6 ContractSite `function-pre`: `__PRETTY_FUNCTION__` names the template parameter, of a precondition.
#ifdef B
template <class U> void f(U) pre(true) {}
template void f<int>(int);
#else
template <class T> void f(T) pre(true) {}
template void f<int>(int);
#endif
