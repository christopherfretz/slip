// std: c++26
#include "side.h"
// §6 ContractSite `function`: `__PRETTY_FUNCTION__` names the template parameter.
#ifdef B
template <class U> void f(U) { contract_assert(true); }
template void f<int>(int);
#else
template <class T> void f(T) { contract_assert(true); }
template void f<int>(int);
#endif
