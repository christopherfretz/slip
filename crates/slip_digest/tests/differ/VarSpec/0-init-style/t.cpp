#include "side.h"
// §5.4 VarSpec `0-init-style`.
#ifdef B
int x{1};
#else
int x = 1;
#endif
