#include "side.h"
// §10.1 (WP-100): a parameter's attribute moved from the first declaration to the definition: the definition's own list is apart, as Kiln's prolog reads it (`class_b.rs:715`).
#ifdef B
void f(int *p);
void f(int *p __attribute__((nonnull))) {}
#else
void f(int *p __attribute__((nonnull)));
void f(int *p) {}
#endif
