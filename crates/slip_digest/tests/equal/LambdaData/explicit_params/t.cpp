#include "side.h"
// §10.4 LambdaData `explicit_params` is `out`: `[]{}` and `[](){}` are one lambda.
#ifdef B
int f(int a, int b) { return []() { return 0; }(); }
#else
int f(int a, int b) { return [] { return 0; }(); }
#endif
