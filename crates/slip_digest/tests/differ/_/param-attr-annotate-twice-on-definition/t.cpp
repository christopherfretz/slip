#include "side.h"
// §10.1 (WP-100): a list's nodes are kept with their multiplicity: `annotate("a")` twice on the definition's parameter against once.
#ifdef B
void f(int *p [[clang::annotate("a")]] [[clang::annotate("a")]]) {}
#else
void f(int *p [[clang::annotate("a")]]) {}
#endif
