#include "side.h"
// §10.4 CaptureKind `StarThis` against `This`.
#ifdef B
struct S { int m; int f() { return [*this] { return m; }(); } };
#else
struct S { int m; int f() { return [this] { return m; }(); } };
#endif
