#include "side.h"
// WP-047: an instantiation that only side A has.
template <class T> T g() { return T(); }
#ifndef B
int f() { return g<int>(); }
#else
int f() { return 0; }
#endif
