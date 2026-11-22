#include "side.h"
// §5.4 VarEvalState `Value.0`, §11.3: the value is the resolved unit's, not the stream's.
#ifdef B
const int x = 2;
int y() { return x; }
#else
const int x = 2;
#endif
