#include "side.h"
// WP-034 (§6): `using T = ns::S;` against `using T = ::ns::S;`.
namespace ns { struct S {}; }
#ifdef B
void f() { using T = ::ns::S; T x; (void)x; }
#else
void f() { using T = ns::S; T x; (void)x; }
#endif
