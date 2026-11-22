#include "side.h"
// WP-047: a use that forces an instantiation (demand).
template <class T> T g() { return T(); }
#ifdef B
int f() { return g<int>(); }
#endif
