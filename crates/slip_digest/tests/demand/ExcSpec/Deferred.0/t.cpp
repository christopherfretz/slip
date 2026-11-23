#include "side.h"
// §4.5 ExcSpec `Deferred.0`, §11.2: the function whose specification is deferred, resolved by a use.
struct S { S(); S(const S &) = default; };
extern S s;
#ifdef B
bool g() { return noexcept(S(s)); }
#endif
