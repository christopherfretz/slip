#include "side.h"
// digest §4.3 `Ref::outer`: a generic lambda's specialisation, a unit of
// its own, names one of two static locals of its owner by its index.
#ifdef B
int f() { static int s = 1, t = 2; return [](auto) { return t; }(0); }
#else
int f() { static int s = 1, t = 2; return [](auto) { return s; }(0); }
#endif
