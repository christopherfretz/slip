#include "side.h"
// §10.4 LambdaData `explicit_result`: a trailing return type naming the deduced type.
#ifdef B
int f(int a, int b) { return []() -> int { return 0; }(); }
#else
int f(int a, int b) { return [] { return 0; }(); }
#endif
