// std: c++17
#include "side.h"
// §5.3 DeclKind `Binding`: added.
struct P { int x, y; } p;
#ifdef B
auto [a, b] = p;
#endif
