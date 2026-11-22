#include "side.h"
// std: c++26
// §6 ContractSite `function`: `__PRETTY_FUNCTION__` prints the parameter's written type.
#ifdef B
typedef int I;
void f(I x) pre(x > 0);
#else
typedef int I;
void f(int x) pre(x > 0);
#endif
