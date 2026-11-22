#include "side.h"
// §5.4 VarEvalState `NotConstant`, §11.3: an attempt that fails.
#ifdef B
int h();
const int x = h();
int y = x;
#else
int h();
const int x = h();
#endif
