#include "side.h"
// digest §14 (WP-045 review probe): a lambda in a default member
// initialiser of a class template, instantiated by a use.
template <class T> struct X { int a = [] { return 1; }(); };
X<int>* p;
#ifdef B
int use() { X<int> x; return x.a; }
#endif
