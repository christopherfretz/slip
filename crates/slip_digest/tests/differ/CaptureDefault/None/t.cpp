#include "side.h"
// §10.4 CaptureDefault `None` against `ByCopy`.
#ifdef B
int f(int a, int b) { return [a] { return a; }(); }
#else
int f(int a, int b) { return [=] { return a; }(); }
#endif
