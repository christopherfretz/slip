#include "side.h"
// WP-155: a function with `used` is emitted unreferenced, a static one
// and an instantiation alike.
__attribute__((used)) static void keep() {}
template <class T> struct K { __attribute__((used)) static void k() {} };
#ifndef B
K<int> *p;
int x = sizeof(&K<int>::k);
#endif
