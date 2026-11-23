#include "side.h"
// §10.2 DeclName `Constructor.0-payload`: the class: a key has no payload (§3.2), the parent key names the class.
#ifdef B
struct T { T(); };
#else
struct S { S(); };
#endif
