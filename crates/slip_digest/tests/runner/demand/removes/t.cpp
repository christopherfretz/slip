#include "side.h"
// WP-047: an instantiation only side A has (not a demand test's shape).
template <class T> T g() { return T(); }
#ifndef B
int f() { return g<int>(); }
#endif
