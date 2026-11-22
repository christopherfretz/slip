#include "side.h"
// §7.2 NonOdrUse `Constant`.
const int c = 1;
#ifdef B
const int &v = c;
#else
int v = c;
#endif
