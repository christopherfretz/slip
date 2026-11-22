#include "side.h"
// std: c++26
// §6 ContractSite `line`: the assertion on another line.
#ifdef B
void f(int x)
    pre(x > 0);
#else
void f(int x) pre(x > 0);
#endif
