#include "side.h"
// §10.4 LambdaCapture `implicit` alone: `this` captured explicitly on side
// A and implicitly on side B, the same kind, default and initialiser.
#ifdef B
struct S { int m; int f() { return [&] { return m; }(); } };
#else
struct S { int m; int f() { return [&, this] { return m; }(); } };
#endif
