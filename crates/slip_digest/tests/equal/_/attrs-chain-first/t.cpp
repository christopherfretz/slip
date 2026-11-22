#include "side.h"
// §10.1 rule: the encoding `attrs`: the same attribute on the first declaration instead.
#ifdef B
__attribute__((cold)) void f();
void f();
#else
void f();
__attribute__((cold)) void f();
#endif
