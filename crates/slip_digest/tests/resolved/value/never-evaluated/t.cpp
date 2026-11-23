#include "side.h"
// digest-kinds §11.3, `NotEvaluated`: no resolved unit on either side.
int z = 3;
#ifdef B
int h() { return z; }
#endif
