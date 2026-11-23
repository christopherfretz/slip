#include "side.h"
// §4.5 ExcSpec `Deferred`, §11.2: a use that resolves the deferred specification of a defaulted constructor changes no unit.
struct S { S() = default; };
#ifdef B
bool g() { return noexcept(S()); }
#endif
