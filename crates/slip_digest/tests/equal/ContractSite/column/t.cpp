#include "side.h"
// std: c++26
// §6 ContractSite `column`: the assertion in another column.
#ifdef B
void f(int x)    pre(x > 0);
#else
void f(int x) pre(x > 0);
#endif
