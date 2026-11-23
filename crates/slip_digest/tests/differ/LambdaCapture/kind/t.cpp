#include "side.h"
// §10.4 LambdaCapture `kind`.
#ifdef B
int f(int a, int b) { return [&a] { return 0; }(); }
#else
int f(int a, int b) { return [a] { return 0; }(); }
#endif
