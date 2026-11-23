#include "side.h"
// §10.2 DeclNameKind `Constructor`: a constructor against a destructor: neither name has a payload.
#ifdef B
struct S { S(); };
#else
struct S { ~S(); };
#endif
