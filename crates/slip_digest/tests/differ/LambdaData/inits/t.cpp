#include "side.h"
// §10.4 LambdaData `inits`: the initialiser of an init-capture.
#ifdef B
int f(int a, int b) { return [x = 2] { return x; }(); }
#else
int f(int a, int b) { return [x = 1] { return x; }(); }
#endif
