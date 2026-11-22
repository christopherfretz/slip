#include "side.h"
// digest §14 demand independence: a block-scope `extern` in a function
// template. Instantiating the function declares `ext` again; Kiln makes
// that declaration an entity of its own, not one of the pattern's chain.
template <class T> int t() { extern int ext; return ext; }
#ifdef B
int use() { return t<int>(); }
#else

#endif
