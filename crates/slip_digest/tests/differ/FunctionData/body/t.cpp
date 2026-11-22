#include "side.h"
// §5.4 FunctionData `body`: two bodies. A definition against a declaration differs in DeclMisc `0-definition` already (differ/DeclMisc/0-definition).
#ifdef B
int f() { return 2; }
#else
int f() { return 1; }
#endif
