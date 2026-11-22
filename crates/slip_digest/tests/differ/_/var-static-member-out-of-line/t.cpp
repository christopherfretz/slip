#include "side.h"
// §2.3: a static data member is read from its out-of-line definition.
#ifdef B
struct C { static int s; }; int C::s = 1;
#else
struct C { static int s; }; int C::s;
#endif
