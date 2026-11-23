#include "side.h"
// digest §4.3: a lambda in the noexcept operand of a lambda, in a body.
#ifdef B
int f() { return [](int a) noexcept([] { return false; }()) { return a; }(1); }
#else
int f() { return [](int a) noexcept([] { return true; }()) { return a; }(1); }
#endif
