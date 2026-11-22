#include "side.h"
// std: c++26
// §5.6 DeclKind `ContractResultName`: a postcondition's result name.
#ifdef B
int f() post(true);
#else
int f() post(r: true);
#endif
