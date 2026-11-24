#include "b.h"
namespace a { struct D : d::B { int f() override { return 1; } }; }
int u(a::D &x) { return x.f(); }
