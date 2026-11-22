#include "side.h"
// §5.4 VarSpec `0-inline`.
#ifdef B
inline int x = 1;
#else
int x = 1;
#endif
