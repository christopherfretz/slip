#include "side.h"
// §2.3, [dcl.fct.default]/4: a default argument given by a block-scope declaration is not the entity's; the block's own is written with its ref (§2.4, WP-026), behind `stmt` here.
#ifdef B
void f(int);
void g() { void f(int = 1); }
#else
void f(int = 1);
void g() { void f(int); }
#endif
