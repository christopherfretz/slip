#include "side.h"
// §10.1 rule: the encoding `attrs`: the stored order is kept.
#ifdef B
__attribute__((noinline, cold)) void f();
#else
__attribute__((cold, noinline)) void f();
#endif
