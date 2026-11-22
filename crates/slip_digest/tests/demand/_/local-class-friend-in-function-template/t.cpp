#include "side.h"
// digest §2.2 (WP-032, check 3): a friend of a local class in a function
// template naming a block-scope declaration: neither the pattern's
// declarations nor the instantiation's make a unit or collide.
template <class T> int t() { void g(int); struct L { friend void g(int); }; return 0; }
#ifdef B
int use() { return t<int>(); }
#endif
