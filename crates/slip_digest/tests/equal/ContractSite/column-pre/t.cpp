// std: c++26
#include "side.h"
// §6 ContractSite `column-pre`: of a precondition.
#ifdef B
void f()     pre(true) {}
#else
void f() pre(true) {}
#endif
