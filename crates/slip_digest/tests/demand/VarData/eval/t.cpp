#include "side.h"
// §5.4 VarData `eval`, §11.3: a use as a constant evaluates the initialiser.
#ifdef B
const int x = 1;
int y = x;
#else
const int x = 1;
#endif
