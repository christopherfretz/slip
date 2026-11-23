#include "side.h"
// §10.1, §5.4 ParamData (WP-100): a parameter's chain is parameter i of every declaration of its function: `__attribute__((nonnull))` on a pointer parameter on a member function declared in its class and defined outside.
#ifdef B
struct S { void f(int *p __attribute__((nonnull))); };
void S::f(int *p) {}
#else
struct S { void f(int *p); };
void S::f(int *p) {}
#endif
