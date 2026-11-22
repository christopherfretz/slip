#include "side.h"
// digest-kinds §11.9: the conversion function of a closure type to a
// pointer to function is defined on use. A closure type is a local
// entity (Area::Local, pending): it has no unit, so no `definition`
// either; the conversion is written where the lambda is (WP-040).
auto l = [](int x) { return x; };
#ifdef B
auto h() -> int (*)(int) { return l; }
#endif
