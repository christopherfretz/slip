#include "side.h"
// WP-155: an instantiation of a variable template whose initialiser is not
// a constant is emitted; one whose initialiser is, is not.
int g();
template <class T> int vt = g();
template <class T> constexpr int vc = sizeof(T);
#ifndef B
int x = vt<int> + vc<int>;
#endif
