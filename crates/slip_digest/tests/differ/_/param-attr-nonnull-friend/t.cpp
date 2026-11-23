#include "side.h"
// §10.1, §5.4 ParamData (WP-100): a parameter's chain is parameter i of every declaration of its function: `__attribute__((nonnull))` on a pointer parameter on a friend declaration in a class, defined outside.
#ifdef B
struct S { friend void f(int *p __attribute__((nonnull))); };
void f(int *p) {}
#else
struct S { friend void f(int *p); };
void f(int *p) {}
#endif
