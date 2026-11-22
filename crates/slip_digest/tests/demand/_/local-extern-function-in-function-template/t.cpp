#include "side.h"
// digest §2.2 (WP-032, check 3): a block-scope function declaration in a
// function template, and a namespace-scope one after it. The pattern's
// is no declaration of `g`'s unit, and the instantiation's (which Kiln
// chains to the namespace one) neither moves nor changes it.
template <class T> int t() { void g(int); return 0; }
void g(int);
#ifdef B
int use() { return t<int>(); }
#endif
