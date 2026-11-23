#include "side.h"
// §10.4 LambdaData `mutable`.
#ifdef B
int f(int a, int b) { return [a]() mutable { return a; }(); }
#else
int f(int a, int b) { return [a]() { return a; }(); }
#endif
