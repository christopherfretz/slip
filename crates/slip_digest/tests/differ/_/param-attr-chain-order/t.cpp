#include "side.h"
// §10.1 (WP-100): a parameter's node moved from the first declaration to the definition is a difference, even where the nodes keep their order over the chain.
#ifdef B
void f(int *p __attribute__((nonnull)) [[clang::annotate("x")]]);
void f(int *p) {}
#else
void f(int *p __attribute__((nonnull)));
void f(int *p [[clang::annotate("x")]]) {}
#endif
