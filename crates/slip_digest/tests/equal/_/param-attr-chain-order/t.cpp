#include "side.h"
// §10.1 (WP-100): a parameter's attributes in chain order, then stored order: the second node moved to the next declaration keeps its place.
#ifdef B
void f(int *p __attribute__((nonnull)) [[clang::annotate("x")]]);
void f(int *p) {}
#else
void f(int *p __attribute__((nonnull)));
void f(int *p [[clang::annotate("x")]]) {}
#endif
