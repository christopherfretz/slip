// std: c++26
#include "side.h"
// §5.4 FnSpec `0-has-contracts`.
#ifdef B
void f(int x) pre(x > 0);
#else
void f(int x);
#endif
