#include "side.h"
// WP-035, digest-kinds §7 `_` `new-requalified`.
namespace ns { struct S { int i; }; }
#ifdef B
ns::S *v = new ::ns::S;
#else
ns::S *v = new ns::S;
#endif
