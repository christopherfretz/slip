#include "side.h"
// §10.4 CaptureDefault `ByRef`: the default with the same explicit capture.
#ifdef B
int f(int a, int b) { return [&, a] { return a; }(); }
#else
int f(int a, int b) { return [a] { return a; }(); }
#endif
