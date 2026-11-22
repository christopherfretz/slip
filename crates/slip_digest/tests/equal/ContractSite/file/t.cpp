#include "side.h"
// std: c++26
// §6 ContractSite `file`: the assertion in another presumed file.
#ifdef B
#line 30 "b.cpp"
void f(int x) pre(x > 0);
#else
#line 30 "a.cpp"
void f(int x) pre(x > 0);
#endif
