#include "side.h"
// digest-kinds §11.9: the conversion function of a closure type to a
// pointer to function is defined on use. A closure type is a local
// type (digest §4.3); its implicit members defined on use are derived
// units under it (WP-040), so the use adds their resolved units: the
// definitions of the conversion function and of the invoker it returns,
// and their exception specifications (§11.2 rule 2).
auto l = [](int x) { return x; };
#ifdef B
auto h() -> int (*)(int) { return l; }
#endif
