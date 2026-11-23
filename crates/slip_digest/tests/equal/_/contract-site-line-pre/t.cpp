// std: c++26
#include "side.h"
// WP-034 (§6): ContractSite, of a precondition.
#ifdef B


void f() pre(true) {}
#else
void f() pre(true) {}
#endif
