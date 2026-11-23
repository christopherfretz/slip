#include "side.h"
// WP-047: an instantiation both sides have, equal; side B adds a variable.
template <class T> T g() { return T(); }
int f() { return g<int>(); }
#ifdef B
int z;
#endif
