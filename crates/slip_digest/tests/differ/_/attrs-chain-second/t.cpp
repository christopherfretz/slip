#include "side.h"
// §10.1 rule: the encoding `attrs`: an attribute on the second declaration is in the unit.
#ifdef B
void f();
__attribute__((cold)) void f();
#else
void f();
void f();
#endif
