#include "side.h"
// §5.4 VarSpec `0-constinit`.
#ifdef B
constinit int x = 1;
#else
int x = 1;
#endif
