#include "side.h"
// WP-047: a use that forces an instantiation (demand).
template <class T> T g(); // declared only: no `g<int>()::<definition>` (WP-045)
#ifdef B
int f() { return g<int>(); }
#endif
