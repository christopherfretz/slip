#include "side.h"
// §10.1 (WP-100): a parameter's attribute moved between two declarations that are not the definition: each non-defining list is kept on its own, the empty one left out.
#ifdef B
void f(int *p);
void f(int *p __attribute__((nonnull)));
void f(int *p) {}
#else
void f(int *p __attribute__((nonnull)));
void f(int *p);
void f(int *p) {}
#endif
