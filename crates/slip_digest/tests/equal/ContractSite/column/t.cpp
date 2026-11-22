// std: c++26
#include "side.h"
// §6 ContractSite `column`.
#ifdef B
void f() {     contract_assert(true); }
#else
void f() { contract_assert(true); }
#endif
