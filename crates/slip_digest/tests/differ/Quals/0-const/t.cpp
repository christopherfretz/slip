#include "side.h"
// §4.3 Quals `0-const`.
#ifdef B
extern int v;
#else
extern const int v;
#endif
