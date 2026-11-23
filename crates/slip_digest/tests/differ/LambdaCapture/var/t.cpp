#include "side.h"
// §10.4 LambdaCapture `var`: another captured variable of the same type.
#ifdef B
int f(int a, int b) { return [b] { return 0; }(); }
#else
int f(int a, int b) { return [a] { return 0; }(); }
#endif
