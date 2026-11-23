#include "side.h"
// §10.4 LambdaData `captures`: their order.
#ifdef B
int f(int a, int b) { return [b, a] { return 0; }(); }
#else
int f(int a, int b) { return [a, b] { return 0; }(); }
#endif
