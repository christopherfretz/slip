#include "side.h"
// §5.4 VarEvalState `Value`, §11.3.
#ifdef B
const int x = 1;
int y = x;
#else
const int x = 1;
#endif
