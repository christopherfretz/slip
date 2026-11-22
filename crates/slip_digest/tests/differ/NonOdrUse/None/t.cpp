#include "side.h"
// §7.2 NonOdrUse `None`.
const int c = 1;
#ifdef B
int v = c;
#else
const int &v = c;
#endif
