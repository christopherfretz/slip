// std: c++26
#include "side.h"
// §6 ContractSite `file-pre`: the presumed file name, of a precondition.
#ifdef B
#line 10 "b.cpp"
void f() pre(true) {}
#else
#line 10 "a.cpp"
void f() pre(true) {}
#endif
