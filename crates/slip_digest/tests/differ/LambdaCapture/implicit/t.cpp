#include "side.h"
// §10.4 LambdaCapture `implicit`: a capture explicit on side A, implicit on side B.
#ifdef B
int f(int a, int b) { return [&] { return a; }(); }
#else
int f(int a, int b) { return [&a] { return a; }(); }
#endif
