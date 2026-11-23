#include "side.h"
// WP-047: an instantiation that only side B has.
template <class T> T g(); // declared only: no `g<int>()::<definition>` (WP-045)
#ifdef B
int f() { return g<int>(); }
#else
int f() { return 0; }
#endif
