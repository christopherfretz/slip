#include "side.h"
// §10.4 CaptureKind `This` against `StarThis`.
#ifdef B
struct S { int m; int f() { return [this] { return m; }(); } };
#else
struct S { int m; int f() { return [*this] { return m; }(); } };
#endif
