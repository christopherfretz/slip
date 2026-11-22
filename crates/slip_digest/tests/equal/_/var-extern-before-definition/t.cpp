#include "side.h"
// §2.3: an `extern` declaration before the definition changes nothing the definition says.
#ifdef B
extern int x; int x = 1;
#else
int x = 1;
#endif
