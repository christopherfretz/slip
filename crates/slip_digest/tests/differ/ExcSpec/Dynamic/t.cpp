// std: c++17
// args: -Wno-dynamic-exception-spec
#include "side.h"
// §4.6 ExcSpec `Dynamic`: `throw(int)` against none (an error in C++17 that `-Wno-dynamic-exception-spec` lets through).
#ifdef B
void f() throw(int);
#else
void f() noexcept(false);
#endif
