#include "side.h"
// §5.3 DeclMisc `0-local-extern`: every declaration at block scope.
#ifdef B
void g() { extern int v; }
#else
extern int v; void g() {}
#endif
