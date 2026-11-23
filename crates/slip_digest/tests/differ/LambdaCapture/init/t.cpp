#include "side.h"
// §10.4 LambdaCapture `init`: an init-capture against a simple capture.
#ifdef B
int f(int a, int b) { return [a = a] { return a; }(); }
#else
int f(int a, int b) { return [a] { return a; }(); }
#endif
