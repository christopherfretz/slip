#include "side.h"
// §10.4: an implicit capture gained by naming one more variable in the body.
#ifdef B
int f(int a, int b) { return [=] { return a + b; }(); }
#else
int f(int a, int b) { return [=] { return a + 0; }(); }
#endif
