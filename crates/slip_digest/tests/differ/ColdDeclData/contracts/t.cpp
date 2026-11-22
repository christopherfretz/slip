// std: c++26
#include "side.h"
// §5.7 ColdDeclData `contracts`: a function's contract assertions.
#ifdef B
void f(int x) pre(x > 0);
#else
void f(int x);
#endif
