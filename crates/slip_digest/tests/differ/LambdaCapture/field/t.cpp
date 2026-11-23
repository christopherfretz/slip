#include "side.h"
// §10.4 LambdaCapture `field`: the capture's field, of another type.
#ifdef B
int f(int a, int b) { return [c = 1L] { return 0; }(); }
#else
int f(int a, int b) { return [a] { return 0; }(); }
#endif
