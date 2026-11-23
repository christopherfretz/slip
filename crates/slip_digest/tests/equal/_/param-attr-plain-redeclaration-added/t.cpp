#include "side.h"
// §10.1 (WP-100): a redeclaration whose parameter has no attributes, added: its empty list is left out.
#ifdef B
void f(int *p __attribute__((nonnull)));
void f(int *p);
void f(int *p [[clang::annotate("x")]]) {}
#else
void f(int *p __attribute__((nonnull)));
void f(int *p [[clang::annotate("x")]]) {}
#endif
