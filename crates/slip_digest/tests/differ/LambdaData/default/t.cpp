#include "side.h"
// §10.4 LambdaData `default`.
#ifdef B
int f(int a, int b) { return [&] { return a; }(); }
#else
int f(int a, int b) { return [=] { return a; }(); }
#endif
