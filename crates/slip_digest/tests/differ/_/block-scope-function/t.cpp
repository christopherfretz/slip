#include "side.h"
// §2.4: a block-scope function declaration is a ref in the body; it is also a declaration of `h`, whose lexical contexts gain `g` (WP-033).
#ifdef B
void h(); void g() { void h(); }
#else
void h(); void g() {}
#endif
