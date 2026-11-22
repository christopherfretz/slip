// std: c++26
#include "side.h"
// §6 ContractSite `file`: the presumed file name.
#ifdef B
#line 10 "b.cpp"
void f() { contract_assert(true); }
#else
#line 10 "a.cpp"
void f() { contract_assert(true); }
#endif
