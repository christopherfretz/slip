#include "side.h"
// §10.1 (WP-100): `annotate` twice on the first declaration against once on it and once on the definition.
#ifdef B
void f(int *p [[clang::annotate("a")]] [[clang::annotate("a")]]);
void f(int *p) {}
#else
void f(int *p [[clang::annotate("a")]]);
void f(int *p [[clang::annotate("a")]]) {}
#endif
