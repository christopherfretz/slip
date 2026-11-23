#include "side.h"
// §10.1 (WP-100): the lists of two non-defining declarations are kept in chain order: `a` then `b` against `b` then `a`.
#ifdef B
void f(int *p [[clang::annotate("b")]]);
void f(int *p [[clang::annotate("a")]]);
void f(int *p) {}
#else
void f(int *p [[clang::annotate("a")]]);
void f(int *p [[clang::annotate("b")]]);
void f(int *p) {}
#endif
