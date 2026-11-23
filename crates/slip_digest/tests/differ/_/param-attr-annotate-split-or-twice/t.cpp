#include "side.h"
// §10.1 (WP-100): `annotate` on a declaration and on the definition against twice on the definition: Kiln emits the definition's own (`annotate.rs:327`), one call against two.
#ifdef B
void f(int *p);
void f(int *p [[clang::annotate("a")]] [[clang::annotate("a")]]) {}
#else
void f(int *p [[clang::annotate("a")]]);
void f(int *p [[clang::annotate("a")]]) {}
#endif
