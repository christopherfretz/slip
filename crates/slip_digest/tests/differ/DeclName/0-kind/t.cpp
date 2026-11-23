#include "side.h"
// §10.2 DeclName `0-kind`: the kind alone differs.
#ifdef B
struct S { S(); };
#else
struct S { ~S(); };
#endif
