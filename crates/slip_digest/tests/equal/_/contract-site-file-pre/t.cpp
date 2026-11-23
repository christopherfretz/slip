// std: c++26
#include "side.h"
// WP-034 (§6): the presumed file name, of a precondition (ContractSite).
#ifdef B
#line 10 "b.cpp"
void f() pre(true) {}
#else
#line 10 "a.cpp"
void f() pre(true) {}
#endif
