#include "side.h"
// WP-034 (§6): `ns::g()` against `g()`.
namespace ns { void g(); }
#ifdef B
namespace ns { void f() { g(); } }
#else
namespace ns { void f() { ns::g(); } }
#endif
