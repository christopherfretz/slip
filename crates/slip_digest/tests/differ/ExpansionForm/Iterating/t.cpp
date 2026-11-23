// std: c++26
#include "side.h"
// §6 ExpansionForm `Iterating`.
static constexpr int a[] = {1, 2};
#ifdef B
void f() { template for (constexpr int e : a) ; }
#else
void f() { template for (constexpr int e : {1, 2}) ; }
#endif
