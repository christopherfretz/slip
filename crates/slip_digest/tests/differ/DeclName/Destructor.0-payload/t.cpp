#include "side.h"
// §10.2 DeclName `Destructor.0-payload`: the class, through the parent key.
#ifdef B
struct T { ~T(); };
#else
struct S { ~S(); };
#endif
