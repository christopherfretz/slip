// std: c++26
#include "side.h"
// §2.3, §5.4 FnSpec `0-has-contracts`: on the declaration with the contract sequence only.
#ifdef B
void f(int x) pre(x > 0);
void f(int x) {}
#else
void f(int x);
void f(int x) {}
#endif
