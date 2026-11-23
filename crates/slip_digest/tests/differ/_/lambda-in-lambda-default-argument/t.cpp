#include "side.h"
// digest §4.3: a lambda in the default argument of a lambda's parameter,
// in a body, is numbered by the body's owner.
#ifdef B
int f() { return [](int a = [] { return 2; }()) { return a; }(); }
#else
int f() { return [](int a = [] { return 1; }()) { return a; }(); }
#endif
