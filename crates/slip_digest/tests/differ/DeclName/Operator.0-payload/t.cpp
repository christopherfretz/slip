#include "side.h"
// §10.2 DeclName `Operator.0-payload`: the operator.
#ifdef B
struct S { S operator-(S); };
#else
struct S { S operator+(S); };
#endif
