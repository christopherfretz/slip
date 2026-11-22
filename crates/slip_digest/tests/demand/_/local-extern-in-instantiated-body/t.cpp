#include "side.h"
// digest §2.2 (WP-032, check 3): a block-scope `extern` in a function
// template. Instantiating the function declares `ext` again, and Kiln
// makes that declaration an entity of its own; neither it nor the
// pattern's declaration is a unit, so side B adds only `use()`.
template <class T> int t() { extern int ext; return ext; }
#ifdef B
int use() { return t<int>(); }
#else

#endif
