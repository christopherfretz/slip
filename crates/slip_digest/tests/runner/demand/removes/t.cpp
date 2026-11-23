#include "side.h"
// WP-047: an instantiation only side A has (not a demand test's shape).
template <class T> T g(); // declared only: no `g<int>()::<definition>` (WP-045)
#ifndef B
int f() { return g<int>(); }
#endif
