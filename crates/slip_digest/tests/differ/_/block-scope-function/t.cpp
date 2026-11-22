#include "side.h"
// §2.4: a block-scope function declaration is a ref in the body; here the body is pending.
#ifdef B
void h(); void g() { void h(); }
#else
void h(); void g() {}
#endif
