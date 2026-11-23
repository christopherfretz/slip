#include "side.h"
// §10.4 CaptureKind `ByRef` against `ByCopy`, the same variable.
#ifdef B
int f(int a, int b) { return [&a] { return 0; }(); }
#else
int f(int a, int b) { return [a] { return 0; }(); }
#endif
