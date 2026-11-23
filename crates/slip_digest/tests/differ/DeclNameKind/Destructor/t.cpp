#include "side.h"
// §10.2 DeclNameKind `Destructor`: a destructor against a constructor.
#ifdef B
struct S { ~S(); };
#else
struct S { S(); };
#endif
