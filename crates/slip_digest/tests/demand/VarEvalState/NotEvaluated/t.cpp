#include "side.h"
// §5.4 VarEvalState `NotEvaluated`, §11.3: never evaluated on side A, evaluated on side B.
#ifdef B
const int x = 1;
int y = x;
#else
const int x = 1;
#endif
