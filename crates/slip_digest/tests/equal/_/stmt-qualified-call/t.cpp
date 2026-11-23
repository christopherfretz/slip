#include "side.h"
// WP-034 (§6): `ns::g()` against `::ns::g()`.
namespace ns { void g(); }
#ifdef B
void f() { ::ns::g(); ns::g(); }
#else
void f() { ns::g(); ::ns::g(); }
#endif
